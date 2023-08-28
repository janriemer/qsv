static USAGE: &str = r#"
Geocodes a location against an updatable local copy of the Geonames cities index.

It has three subcommands:
 * suggest - given a City name, return the closest location coordinate.
 * reverse - given a location coordinate, return the closest City.
 * index - operations to update the Geonames cities index used by the geocode command.
 
SUGGEST
Geocodes to the nearest city center point given a location column
[i.e. a column which contains a latitude, longitude WGS84 coordinate] against
an embedded copy of the Geonames city index.

The geocoded information is formatted based on --formatstr, returning
it in 'city-state' format if not specified.

Use the --new-column option if you want to keep the location column:

Examples:
Geocode file.csv Location column and set the geocoded value to a
new column named City.

$ qsv geocode suggest Location --new-column City file.csv

Geocode file.csv Location column with --formatstr=city-state and
set the geocoded value a new column named City.

$ qsv geocode suggest Location --formatstr city-state --new-column City file.csv

REVERSE
Reverse geocode a WG84 coordinate to the nearest city center point.

Examples:
Reverse geocode file.csv LatLong column and set the geocoded value to a
new column named City.

$ qsv geocode reverse LatLong --new-column City file.csv

INDEX-<operation>
Updates the Geonames cities index used by the geocode command.

It has four operations:
 * check - checks if the local Geonames index is up-to-date.
 * update - updates the local Geonames index with the latest changes from the Geonames website.
 * reset - resets the local Geonames index to the default Geonames cities index, downloading
           it from the qsv GitHub repo for that release.
 * load - load a Geonames cities index from a file, making it the default index from that point
          forward.

Examples:
Update the Geonames cities index with the latest changes.

$ qsv geocode index-update

Load a Geonames cities index from a file.

$ qsv geocode index-load my_geonames_index.bincode

For more extensive examples, see https://github.com/jqnatividad/qsv/blob/master/tests/test_geocode.rs.

Usage:
qsv geocode suggest [--formatstr=<string>] [options] <column> [<input>]
qsv geocode reverse [--formatstr=<string>] [options] <column> [<input>]
qsv geocode index-load <index-file>
qsv geocode index-check
qsv geocode index-update
qsv geocode index-reset
qsv geocode --help

geocode arguments:
        
    <input>                     The input file to read from. If not specified, reads from stdin.
    <column>                    The column to geocode.
    <index-file>                The alternate geonames index file to use. It must be a .bincode file.
                                Only used by the 'load' operations.

geocode options:
    -c, --new-column <name>     Put the transformed values in a new column instead.
    -r, --rename <name>         New name for the transformed column.
    -f, --formatstr=<string>    This option is used by several subcommands:

                                The place format to use. The available formats are:
                                  - 'city-state' (default) - e.g. Brooklyn, New York
                                  - 'city-country' - Brooklyn, US
                                  - 'city-state-country' | 'city-admin1-country' - Brooklyn, New York US
                                  - 'city' - Brooklyn
                                  - 'county' | 'admin2' - Kings County
                                  - 'state' | 'admin1' - New York
                                  - 'county-country' | 'admin2-country' - Kings County, US
                                  - 'county-state-country' | 'admin2-admin1-country' - Kings County, New York US
                                  - 'country' - US
    -j, --jobs <arg>            The number of jobs to run in parallel.
                                When not set, the number of jobs is set to the number of CPUs detected.
    -b, --batch <size>          The number of rows per batch to load into memory, before running in parallel.
                                [default: 50000]
    --timeout <seconds>         Timeout for downloading Geonames cities index.
                                [default: 60]
    --languages <lang>          The languages to use for the Geonames cities index.
                                The languages are specified as a comma-separated list of ISO 639-1 codes.
                                [default: en]
    --cache-dir <dir>           The directory to use for caching the Geonames cities index.
                                If the directory does not exist, qsv will attempt to create it.
                                If the QSV_CACHE_DIR envvar is set, it will be used instead.
                                [default: qsv-cache]

Common options:
    -h, --help                  Display this message
    -o, --output <file>         Write output to <file> instead of stdout.
    -d, --delimiter <arg>       The field delimiter for reading CSV data.
                                Must be a single character. (default: ,)
    -p, --progressbar           Show progress bars. Not valid for stdin.
"#;

use std::{
    fs,
    path::{Path, PathBuf},
};

use cached::proc_macro::cached;
use geosuggest_core::{Engine, EngineDumpFormat};
use geosuggest_utils::{IndexUpdater, IndexUpdaterSettings, SourceItem};
use indicatif::{ProgressBar, ProgressDrawTarget};
use log::{debug, info};
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    prelude::IntoParallelRefIterator,
};
use regex::Regex;
use serde::Deserialize;

use crate::{
    clitypes::CliError,
    config::{Config, Delimiter},
    regex_oncelock,
    select::SelectColumns,
    util, CliResult,
};

#[derive(Deserialize, Debug)]
struct Args {
    arg_column:       String,
    cmd_suggest:      bool,
    cmd_reverse:      bool,
    cmd_index_check:  bool,
    cmd_index_update: bool,
    cmd_index_load:   bool,
    cmd_index_reset:  bool,
    arg_input:        Option<String>,
    arg_index_file:   Option<String>,
    flag_rename:      Option<String>,
    flag_formatstr:   String,
    flag_batch:       u32,
    flag_timeout:     u16,
    flag_languages:   String,
    flag_cache_dir:   String,
    flag_jobs:        Option<usize>,
    flag_new_column:  Option<String>,
    flag_output:      Option<String>,
    flag_delimiter:   Option<Delimiter>,
    flag_progressbar: bool,
}

static DEFAULT_GEOCODE_INDEX_FILENAME: &str = "qsv-geocode-index.bincode";

static DEFAULT_CITIES_DB_URL: &str = "https://download.geonames.org/export/dump/cities15000.zip";
static DEFAULT_CITIES_DB_FILENAME: &str = "cities15000.txt";
static DEFAULT_CITIES_NAMES_URL: &str =
    "https://download.geonames.org/export/dump/alternateNamesV2.zip";
static DEFAULT_CITIES_NAMES_FILENAME: &str = "alternateNamesV2.txt";
static DEFAULT_COUNTRY_INFO_URL: &str = "https://download.geonames.org/export/dump/countryInfo.txt";
static DEFAULT_ADMIN1_CODES_URL: &str =
    "https://download.geonames.org/export/dump/admin1CodesASCII.txt";

// valid subcommands
#[derive(Clone, Copy, PartialEq)]
enum GeocodeSubCmd {
    Suggest,
    Reverse,
    IndexCheck,
    IndexUpdate,
    IndexLoad,
    IndexReset,
}

// we need this as geosuggest uses anyhow::Error
impl From<anyhow::Error> for CliError {
    fn from(err: anyhow::Error) -> CliError {
        CliError::Other(format!("Error: {err}"))
    }
}

#[inline]
fn replace_column_value(
    record: &csv::StringRecord,
    column_index: usize,
    new_value: &str,
) -> csv::StringRecord {
    record
        .into_iter()
        .enumerate()
        .map(|(i, v)| if i == column_index { new_value } else { v })
        .collect()
}

pub fn run(argv: &[&str]) -> CliResult<()> {
    let args: Args = util::get_args(USAGE, argv)?;

    // we need to use tokio runtime as geosuggest uses async
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(geocode_main(args))?;

    Ok(())
}

// main async geocode function that does the actual work
async fn geocode_main(args: Args) -> CliResult<()> {
    eprintln!("args: {args:?}");

    let mut index_cmd = true;

    let geocode_cmd = if args.cmd_suggest {
        index_cmd = false;
        GeocodeSubCmd::Suggest
    } else if args.cmd_reverse {
        index_cmd = false;
        GeocodeSubCmd::Reverse
    } else if args.cmd_index_check {
        GeocodeSubCmd::IndexCheck
    } else if args.cmd_index_update {
        GeocodeSubCmd::IndexUpdate
    } else if args.cmd_index_load {
        GeocodeSubCmd::IndexLoad
    } else if args.cmd_index_reset {
        GeocodeSubCmd::IndexReset
    } else {
        return fail_incorrectusage_clierror!("Unknown geocode subcommand.");
    };

    // setup cache directory
    let geocode_cache_dir = if let Ok(cache_dir) = std::env::var("QSV_CACHE_DIR") {
        // if QSV_CACHE_DIR env var is set, check if it exists. If it doesn't, create it.
        if !Path::new(&cache_dir).exists() {
            fs::create_dir_all(&cache_dir)?;
        }
        cache_dir
    } else {
        if !Path::new(&args.flag_cache_dir).exists() {
            fs::create_dir_all(&args.flag_cache_dir)?;
        }
        args.flag_cache_dir.clone()
    };
    info!("Using cache directory: {geocode_cache_dir}");

    let geocode_index_filename = std::env::var("QSV_GEOCODE_INDEX_FILENAME")
        .unwrap_or_else(|_| DEFAULT_GEOCODE_INDEX_FILENAME.to_string());
    let geocode_index_file = args.arg_index_file.clone().unwrap_or_else(|| {
        let mut path = PathBuf::from(geocode_cache_dir);
        path.push(geocode_index_filename);
        path.to_string_lossy().to_string()
    });

    // setup languages
    let languages_string_vec = args
        .flag_languages
        .to_ascii_lowercase()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<Vec<String>>();
    let languages_vec: Vec<&str> = languages_string_vec
        .iter()
        .map(std::string::String::as_str)
        .collect();

    debug!("geocode_index_file: {geocode_index_file} Languages: {languages_vec:?}");

    let updater = IndexUpdater::new(IndexUpdaterSettings {
        http_timeout_ms:  util::timeout_secs(args.flag_timeout)? * 1000,
        cities:           SourceItem {
            url:      DEFAULT_CITIES_DB_URL,
            filename: DEFAULT_CITIES_DB_FILENAME,
        },
        names:            Some(SourceItem {
            url:      DEFAULT_CITIES_NAMES_URL,
            filename: DEFAULT_CITIES_NAMES_FILENAME,
        }),
        countries_url:    Some(DEFAULT_COUNTRY_INFO_URL),
        admin1_codes_url: Some(DEFAULT_ADMIN1_CODES_URL),
        filter_languages: languages_vec.clone(),
    })?;

    if index_cmd {
        match geocode_cmd {
            GeocodeSubCmd::IndexCheck => {
                // load geocode engine
                winfo!("Checking main Geonames website for updates...");
                let engine =
                    load_engine(geocode_index_file.clone().into(), args.flag_progressbar).await?;

                if updater.has_updates(&engine).await? {
                    winfo!("Updates available. Use `qsv geocode index-update` to apply.");
                } else {
                    winfo!("Geonames index up-to-date.");
                }
            },
            GeocodeSubCmd::IndexUpdate => {
                winfo!("Updating Geonames index. This will take a while...");
                let engine = updater.build().await?;
                engine.dump_to(geocode_index_file.clone(), EngineDumpFormat::Bincode)?;
                winfo!("Updates applied: {geocode_index_file}");
            },
            GeocodeSubCmd::IndexLoad => {
                // load alternate geocode index file
                if let Some(index_file) = args.arg_index_file {
                    winfo!("Validating alternate Geonames index: {index_file}...");
                    // check if index_file ends with a .bincode extension
                    if !index_file.ends_with(".bincode") {
                        return fail_incorrectusage_clierror!(
                            "Alternate Geonames index file {index_file} does not have a .bincode \
                             extension."
                        );
                    }
                    // check if index_file exist
                    if !Path::new(&index_file).exists() {
                        return fail_incorrectusage_clierror!(
                            "Alternate Geonames index file {index_file} does not exist."
                        );
                    }

                    let engine = load_engine(index_file.clone().into(), true).await?;
                    // we successfully loaded the alternate geocode index file, so its valid
                    // copy it to the default geocode index file
                    engine.dump_to(geocode_index_file.clone(), EngineDumpFormat::Bincode)?;
                    winfo!(
                        "Valid Geonames index file {index_file} copied to {geocode_index_file}. \
                         It will be used from now on or until you reset it.",
                    );
                } else {
                    return fail_incorrectusage_clierror!(
                        "No alternate Geonames index file specified."
                    );
                }
            },
            GeocodeSubCmd::IndexReset => {
                // reset geocode index to the default geocode index by deleting the current one
                // the load_engine() function will then download the default geocode index
                // from the qsv GitHub repo the next time it's called
                winfo!("Resetting Geonames index to default...");
                if Path::new(&geocode_index_file).exists() {
                    fs::remove_file(&geocode_index_file)?;
                }
            },
            _ => unreachable!("index_cmd is true, so this is unreachable."),
        }
        return Ok(());
    }

    // we're not doing an index subcommand, so we're doing a suggest or reverse
    let engine = load_engine(geocode_index_file.clone().into(), args.flag_progressbar).await?;

    let rconfig = Config::new(&args.arg_input)
        .delimiter(args.flag_delimiter)
        .select(SelectColumns::parse("").unwrap()); // select all columns

    if rconfig.is_stdin() {
        // is_stdin is being used, check if args.arg_column is a file that exists
        // if it does, then we need to trap its as an error as docopt gets confused and
        // will set arg_column to the input file when reading from stdin.
        if let Ok(path) = Path::new(&args.arg_column).canonicalize() {
            if path.exists() {
                return fail_incorrectusage_clierror!("No/incorrect column specified.");
            }
        }
    }

    let mut rdr = rconfig.reader()?;
    let mut wtr = Config::new(&args.flag_output).writer()?;

    let headers = rdr.byte_headers()?.clone();
    let sel = rconfig.selection(&headers)?;
    let column_index = *sel.iter().next().unwrap();

    let mut headers = rdr.headers()?.clone();

    if let Some(new_name) = args.flag_rename {
        let new_col_names = util::ColumnNameParser::new(&new_name).parse()?;
        if new_col_names.len() != sel.len() {
            return fail_incorrectusage_clierror!(
                "Number of new columns does not match input column selection."
            );
        }
        for (i, col_index) in sel.iter().enumerate() {
            headers = replace_column_value(&headers, *col_index, &new_col_names[i]);
        }
    }

    if let Some(new_column) = &args.flag_new_column {
        headers.push_field(new_column);
    }
    wtr.write_record(&headers)?;

    // prep progress bar
    let show_progress =
        (args.flag_progressbar || util::get_envvar_flag("QSV_PROGRESSBAR")) && !rconfig.is_stdin();

    let progress = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr_with_hz(5));
    if show_progress {
        util::prep_progress(&progress, util::count_rows(&rconfig)?);
    } else {
        progress.set_draw_target(ProgressDrawTarget::hidden());
    }

    // amortize memory allocation by reusing record
    #[allow(unused_assignments)]
    let mut batch_record = csv::StringRecord::new();

    // reuse batch buffers
    let batchsize: usize = args.flag_batch as usize;
    let mut batch = Vec::with_capacity(batchsize);
    let mut batch_results = Vec::with_capacity(batchsize);

    // set RAYON_NUM_THREADS
    util::njobs(args.flag_jobs);

    // main loop to read CSV and construct batches for parallel processing.
    // each batch is processed via Rayon parallel iterator.
    // loop exits when batch is empty.
    'batch_loop: loop {
        for _ in 0..batchsize {
            match rdr.read_record(&mut batch_record) {
                Ok(has_data) => {
                    if has_data {
                        batch.push(batch_record.clone());
                    } else {
                        // nothing else to add to batch
                        break;
                    }
                },
                Err(e) => {
                    return fail_clierror!("Error reading file: {e}");
                },
            }
        }

        if batch.is_empty() {
            // break out of infinite loop when at EOF
            break 'batch_loop;
        }

        // do actual apply command via Rayon parallel iterator
        batch
            .par_iter()
            .map(|record_item| {
                let mut record = record_item.clone();
                let mut cell = record[column_index].to_owned();
                if !cell.is_empty() {
                    let search_result =
                        search_cached(&engine, geocode_cmd, &cell, &args.flag_formatstr);
                    if let Some(geocoded_result) = search_result {
                        cell = geocoded_result;
                    }
                }
                if args.flag_new_column.is_some() {
                    record.push_field(&cell);
                } else {
                    record = replace_column_value(&record, column_index, &cell);
                }

                record
            })
            .collect_into_vec(&mut batch_results);

        // rayon collect() guarantees original order, so we can just append results each batch
        for result_record in &batch_results {
            wtr.write_record(result_record)?;
        }

        if show_progress {
            progress.inc(batch.len() as u64);
        }

        batch.clear();
    } // end batch loop

    if show_progress {
        util::update_cache_info!(progress, SEARCH_CACHED);
        util::finish_progress(&progress);
    }
    Ok(wtr.flush()?)
}

async fn load_engine(geocode_index_file: PathBuf, show_progress: bool) -> CliResult<Engine> {
    let index_file = std::path::Path::new(&geocode_index_file);

    if index_file.exists() {
        // load existing local index
        if show_progress {
            woutinfo!(
                "Loading existing geocode index from {}",
                index_file.display()
            );
        }
    } else {
        let qsv_version = env!("CARGO_PKG_VERSION");

        // initial load, download index file from qsv releases
        if show_progress {
            woutinfo!(
                "No local index found. Downloading geocode index from qsv {qsv_version} release..."
            );
        }
        util::download_file(
            &format!(
                "https://github.com/jqnatividad/qsv/releases/tag/{qsv_version}/qsv-geocode-index.bincode"
            ),
            &geocode_index_file.to_string_lossy(),
            show_progress,
            None,
            None,
            None,
        )
        .await?;
    }
    let engine = Engine::load_from(index_file, EngineDumpFormat::Bincode)
        .map_err(|e| format!("On load index file: {e}"))?;
    Ok(engine)
}

#[cached(
    key = "String",
    convert = r#"{ format!("{cell}") }"#,
    option = true,
    sync_writes = true
)]
fn search_cached(
    engine: &Engine,
    mode: GeocodeSubCmd,
    cell: &str,
    formatstr: &str,
) -> Option<String> {
    static EMPTY_STRING: String = String::new();

    let mut id = 0_usize;
    let mut city_name = String::new();
    let mut country = String::new();
    let mut admin1_name_value = String::new();
    let mut latitude = 0_f32;
    let mut longitude = 0_f32;
    let mut population = 0_usize;
    let mut timezone = String::new();
    let mut cityrecord_dbg = String::new();

    if mode == GeocodeSubCmd::Suggest {
        let search_result = engine.suggest(cell, 1, None);
        let Some(cityrecord) = search_result.into_iter().next() else {
            return None;
        };

        let Some((_admin1_name_key, admin1_name_value_work)) = (match &cityrecord.admin1_names {
            Some(admin1) => admin1.iter().next().map(|s| s.to_owned()),
            None => Some((&EMPTY_STRING, &EMPTY_STRING)),
        }) else {
            return None;
        };

        id = cityrecord.id;
        city_name = cityrecord.name.clone();
        latitude = cityrecord.latitude;
        longitude = cityrecord.longitude;
        country = cityrecord.country.clone().unwrap().name;
        admin1_name_value = admin1_name_value_work.clone();
        population = cityrecord.population;
        timezone = cityrecord.timezone.clone();
        cityrecord_dbg = if formatstr == "cityrecord" {
            format!("{cityrecord:?}")
        } else {
            EMPTY_STRING.clone()
        };
    } else if mode == GeocodeSubCmd::Reverse {
        // regex for Location field. Accepts (lat, long) & lat, long
        let locregex: &'static Regex = regex_oncelock!(
            r"(?-u)([+-]?[0-9]+\.?[0-9]*|\.[0-9]+),\s*([+-]?[0-9]+\.?[0-9]*|\.[0-9]+)"
        );

        let loccaps = locregex.captures(cell);
        if let Some(loccaps) = loccaps {
            let lat = fast_float::parse(&loccaps[1]).unwrap_or_default();
            let long = fast_float::parse(&loccaps[2]).unwrap_or_default();
            if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&long) {
                let search_result = engine.reverse((lat, long), 1, None);
                let Some(cityrecord) = (match search_result {
                    Some(search_result) => search_result.into_iter().next().map(|ri| ri.city),
                    None => return None,
                }) else {
                    return None;
                };

                let Some((_admin1_name_key, admin1_name_value_work)) =
                    (match &cityrecord.admin1_names {
                        Some(admin1) => admin1.iter().next().map(|s| s.to_owned()),
                        None => Some((&EMPTY_STRING, &EMPTY_STRING)),
                    })
                else {
                    return None;
                };

                id = cityrecord.id;
                city_name = cityrecord.name.clone();
                latitude = cityrecord.latitude;
                longitude = cityrecord.longitude;
                country = cityrecord.country.clone().unwrap().name;
                admin1_name_value = admin1_name_value_work.clone();
                population = cityrecord.population;
                timezone = cityrecord.timezone.clone();
                cityrecord_dbg = if formatstr == "cityrecord" {
                    format!("{cityrecord:?}")
                } else {
                    EMPTY_STRING.clone()
                };
            }
        } else {
            return None;
        }
    } else {
        return None;
    }

    #[allow(clippy::match_same_arms)]
    // match arms are evaluated in order,
    // so we're optimizing for the most common cases first
    let result = match formatstr {
        "%+" | "city-state" => format!("{city_name}, {admin1_name_value}"),
        "lat-long" => format!("{latitude}, {longitude}"),
        "location" => format!("({latitude}, {longitude})"),
        "city-country" => format!("{city_name}, {country}"),
        "city" => city_name,
        "state" => admin1_name_value,
        "country" => country,
        "id" => format!("{id}"),
        "population" => format!("{population}"),
        "timezone" => timezone,
        "cityrecord" => cityrecord_dbg,
        _ => format!("{city_name}, {admin1_name_value}, {country}"),
    };
    return Some(result);
}