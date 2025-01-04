use crate::workdir::Workdir;

// This macro takes *two* identifiers: one for the test with headers
// and another for the test without headers.
macro_rules! join_test {
    ($name:ident, $fun:expr) => {
        mod $name {
            use std::process;

            use super::{make_rows, setup};
            use crate::workdir::Workdir;

            #[test]
            fn headers() {
                let wrk = setup(stringify!($name), true);
                let mut cmd = wrk.command("join");
                cmd.args(&["city", "cities.csv", "city", "places.csv"]);
                $fun(wrk, cmd, true);
            }

            #[test]
            fn no_headers() {
                let n = stringify!(concat_idents!($name, _no_headers));
                let wrk = setup(n, false);
                let mut cmd = wrk.command("join");
                cmd.arg("--no-headers");
                cmd.args(&["1", "cities.csv", "1", "places.csv"]);
                $fun(wrk, cmd, false);
            }
        }
    };
}

fn setup(name: &str, headers: bool) -> Workdir {
    let mut cities = vec![
        svec!["Boston", "MA"],
        svec!["New York", "NY"],
        svec!["San Francisco", "CA"],
        svec!["Buffalo", "NY"],
    ];
    let mut places = vec![
        svec!["Boston", "Logan Airport"],
        svec!["Boston", "Boston Garden"],
        svec!["Buffalo", "Ralph Wilson Stadium"],
        svec!["Orlando", "Disney World"],
        svec!("BOSTON", "BOSTON COMMON"),
    ];
    if headers {
        cities.insert(0, svec!["city", "state"]);
    }
    if headers {
        places.insert(0, svec!["city", "place"]);
    }

    let wrk = Workdir::new(name);
    wrk.create("cities.csv", cities);
    wrk.create("places.csv", places);
    wrk
}

fn make_rows(headers: bool, left_only: bool, rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut all_rows = vec![];
    if headers {
        if left_only {
            all_rows.push(svec!["city", "state"]);
        } else {
            all_rows.push(svec!["city", "state", "city", "place"]);
        }
    }
    all_rows.extend(rows.into_iter());
    all_rows
}

join_test!(join_inner, |wrk: Workdir,
                        mut cmd: process::Command,
                        headers: bool| {
    let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
    let expected = make_rows(
        headers,
        false,
        vec![
            svec!["Boston", "MA", "Boston", "Logan Airport"],
            svec!["Boston", "MA", "Boston", "Boston Garden"],
            svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
        ],
    );
    assert_eq!(got, expected);
});

join_test!(
    join_inner_casei,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--ignore-case");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            false,
            vec![
                svec!["Boston", "MA", "Boston", "Logan Airport"],
                svec!["Boston", "MA", "Boston", "Boston Garden"],
                svec!["Boston", "MA", "BOSTON", "BOSTON COMMON"],
                svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_outer_left,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--left");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            false,
            vec![
                svec!["Boston", "MA", "Boston", "Logan Airport"],
                svec!["Boston", "MA", "Boston", "Boston Garden"],
                svec!["New York", "NY", "", ""],
                svec!["San Francisco", "CA", "", ""],
                svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_outer_left_casei,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--left").arg("--ignore-case");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            false,
            vec![
                svec!["Boston", "MA", "Boston", "Logan Airport"],
                svec!["Boston", "MA", "Boston", "Boston Garden"],
                svec!["Boston", "MA", "BOSTON", "BOSTON COMMON"],
                svec!["New York", "NY", "", ""],
                svec!["San Francisco", "CA", "", ""],
                svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_outer_right,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--right");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            false,
            vec![
                svec!["Boston", "MA", "Boston", "Logan Airport"],
                svec!["Boston", "MA", "Boston", "Boston Garden"],
                svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
                svec!["", "", "Orlando", "Disney World"],
                svec!["", "", "BOSTON", "BOSTON COMMON"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_outer_right_casei,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--right").arg("--ignore-case");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            false,
            vec![
                svec!["Boston", "MA", "Boston", "Logan Airport"],
                svec!["Boston", "MA", "Boston", "Boston Garden"],
                svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
                svec!["", "", "Orlando", "Disney World"],
                svec!["Boston", "MA", "BOSTON", "BOSTON COMMON"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_outer_full,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--full");

        wrk.assert_success(&mut cmd);

        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            false,
            vec![
                svec!["Boston", "MA", "Boston", "Logan Airport"],
                svec!["Boston", "MA", "Boston", "Boston Garden"],
                svec!["New York", "NY", "", ""],
                svec!["San Francisco", "CA", "", ""],
                svec!["Buffalo", "NY", "Buffalo", "Ralph Wilson Stadium"],
                svec!["", "", "Orlando", "Disney World"],
                svec!["", "", "BOSTON", "BOSTON COMMON"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(join_left_semi, |wrk: Workdir,
                            mut cmd: process::Command,
                            headers: bool| {
    cmd.arg("--left-semi");
    let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
    let expected = make_rows(
        headers,
        true,
        vec![svec!["Boston", "MA"], svec!["Buffalo", "NY"]],
    );
    assert_eq!(got, expected);
});

join_test!(join_left_anti, |wrk: Workdir,
                            mut cmd: process::Command,
                            headers: bool| {
    cmd.arg("--left-anti");
    let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
    let expected = make_rows(
        headers,
        true,
        vec![svec!["New York", "NY"], svec!["San Francisco", "CA"]],
    );
    assert_eq!(got, expected);
});

#[test]
fn join_inner_issue11() {
    let a = vec![svec!["1", "2"], svec!["3", "4"], svec!["5", "6"]];
    let b = vec![svec!["2", "1"], svec!["4", "3"], svec!["6", "5"]];

    let wrk = Workdir::new("join_inner_issue11");
    wrk.create("a.csv", a);
    wrk.create("b.csv", b);

    let mut cmd = wrk.command("join");
    cmd.args(["1,2", "a.csv", "2,1", "b.csv"]);

    let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
    let expected = vec![
        svec!["1", "2", "2", "1"],
        svec!["3", "4", "4", "3"],
        svec!["5", "6", "6", "5"],
    ];
    assert_eq!(got, expected);
}

#[test]
fn join_cross() {
    let wrk = Workdir::new("join_cross");
    wrk.create(
        "letters.csv",
        vec![svec!["h1", "h2"], svec!["a", "b"], svec!["c", "d"]],
    );
    wrk.create(
        "numbers.csv",
        vec![svec!["h3", "h4"], svec!["1", "2"], svec!["3", "4"]],
    );

    let mut cmd = wrk.command("join");
    cmd.arg("--cross")
        .args(["", "letters.csv", "", "numbers.csv"]);
    let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
    let expected = vec![
        svec!["h1", "h2", "h3", "h4"],
        svec!["a", "b", "1", "2"],
        svec!["a", "b", "3", "4"],
        svec!["c", "d", "1", "2"],
        svec!["c", "d", "3", "4"],
    ];
    assert_eq!(got, expected);
}

#[test]
fn join_cross_no_headers() {
    let wrk = Workdir::new("join_cross_no_headers");
    wrk.create("letters.csv", vec![svec!["a", "b"], svec!["c", "d"]]);
    wrk.create("numbers.csv", vec![svec!["1", "2"], svec!["3", "4"]]);

    let mut cmd = wrk.command("join");
    cmd.arg("--cross")
        .arg("--no-headers")
        .args(["", "letters.csv", "", "numbers.csv"]);
    let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
    let expected = vec![
        svec!["a", "b", "1", "2"],
        svec!["a", "b", "3", "4"],
        svec!["c", "d", "1", "2"],
        svec!["c", "d", "3", "4"],
    ];
    assert_eq!(got, expected);
}

join_test!(
    join_right_semi,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--right-semi");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            true,
            vec![
                svec!["Boston", "Logan Airport"],
                svec!["Boston", "Boston Garden"],
                svec!["Buffalo", "Ralph Wilson Stadium"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_right_semi_casei,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--right-semi").arg("--ignore-case");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            true,
            vec![
                svec!["Boston", "Logan Airport"],
                svec!["Boston", "Boston Garden"],
                svec!["Buffalo", "Ralph Wilson Stadium"],
                svec!["BOSTON", "BOSTON COMMON"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_right_anti,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--right-anti");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(
            headers,
            true,
            vec![
                svec!["Orlando", "Disney World"],
                svec!["BOSTON", "BOSTON COMMON"],
            ],
        );
        assert_eq!(got, expected);
    }
);

join_test!(
    join_right_anti_casei,
    |wrk: Workdir, mut cmd: process::Command, headers: bool| {
        cmd.arg("--right-anti").arg("--ignore-case");
        let got: Vec<Vec<String>> = wrk.read_stdout(&mut cmd);
        let expected = make_rows(headers, true, vec![svec!["Orlando", "Disney World"]]);
        assert_eq!(got, expected);
    }
);

#[test]
fn join_keys_output_inner() {
    let wrk = Workdir::new("join_keys_inner");
    wrk.create(
        "letters.csv",
        vec![
            svec!["letter", "value"],
            svec!["a", "1"],
            svec!["b", "2"],
            svec!["c", "3"],
        ],
    );
    wrk.create(
        "numbers.csv",
        vec![
            svec!["letter", "num"],
            svec!["b", "foo"],
            svec!["c", "bar"],
            svec!["d", "baz"],
        ],
    );

    let mut cmd = wrk.command("join");
    cmd.args(["letter", "letters.csv", "letter", "numbers.csv"])
        .arg("--keys-output")
        .arg("keys.csv");

    wrk.run(&mut cmd);

    let got: Vec<Vec<String>> = wrk
        .read_to_string("keys.csv")
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(',').map(String::from).collect())
        .collect();
    let expected = vec![svec!["b"], svec!["c"]];
    assert_eq!(got, expected);
}

#[test]
fn join_keys_output_left_anti() {
    let wrk = Workdir::new("join_keys_left_anti");
    wrk.create(
        "letters.csv",
        vec![
            svec!["letter", "value"],
            svec!["a", "1"],
            svec!["b", "2"],
            svec!["c", "3"],
        ],
    );
    wrk.create(
        "numbers.csv",
        vec![
            svec!["letter", "num"],
            svec!["b", "foo"],
            svec!["c", "bar"],
            svec!["d", "baz"],
        ],
    );

    let mut cmd = wrk.command("join");
    cmd.args(["letter", "letters.csv", "letter", "numbers.csv"])
        .arg("--left-anti")
        .arg("--keys-output")
        .arg("keys.csv");

    wrk.run(&mut cmd);

    let got: Vec<Vec<String>> = wrk
        .read_to_string("keys.csv")
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(',').map(String::from).collect())
        .collect();
    let expected = vec![svec!["a"]]; // Only 'a' has no match in numbers.csv
    assert_eq!(got, expected);
}

#[test]
fn join_keys_output_left_semi() {
    let wrk = Workdir::new("join_keys_left_semi");
    wrk.create(
        "letters.csv",
        vec![
            svec!["letter", "value"],
            svec!["a", "1"],
            svec!["b", "2"],
            svec!["c", "3"],
        ],
    );
    wrk.create(
        "numbers.csv",
        vec![
            svec!["letter", "num"],
            svec!["b", "foo"],
            svec!["c", "bar"],
            svec!["d", "baz"],
        ],
    );

    let mut cmd = wrk.command("join");
    cmd.args(["letter", "letters.csv", "letter", "numbers.csv"])
        .arg("--left-semi")
        .arg("--keys-output")
        .arg("keys.csv");

    wrk.run(&mut cmd);

    let got: Vec<Vec<String>> = wrk
        .read_to_string("keys.csv")
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(',').map(String::from).collect())
        .collect();
    let expected = vec![svec!["b"], svec!["c"]]; // 'b' and 'c' have matches
    assert_eq!(got, expected);
}

#[test]
fn join_keys_output_full() {
    let wrk = Workdir::new("join_keys_full");
    wrk.create(
        "letters.csv",
        vec![
            svec!["letter", "value"],
            svec!["a", "1"],
            svec!["b", "2"],
            svec!["c", "3"],
        ],
    );
    wrk.create(
        "numbers.csv",
        vec![
            svec!["letter", "num"],
            svec!["b", "foo"],
            svec!["c", "bar"],
            svec!["d", "baz"],
        ],
    );

    let mut cmd = wrk.command("join");
    cmd.args(["letter", "letters.csv", "letter", "numbers.csv"])
        .arg("--full")
        .arg("--keys-output")
        .arg("keys.csv");

    wrk.run(&mut cmd);

    let got: Vec<Vec<String>> = wrk
        .read_to_string("keys.csv")
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(',').map(String::from).collect())
        .collect();
    let expected = vec![svec!["b"], svec!["c"]]; // Only matched keys are written
    assert_eq!(got, expected);
}

#[test]
fn join_keys_output_multiple_columns() {
    let wrk = Workdir::new("join_keys_multiple");
    wrk.create(
        "data1.csv",
        vec![
            svec!["city", "state", "val"],
            svec!["Boston", "MA", "1"],
            svec!["New York", "NY", "2"],
            svec!["Chicago", "IL", "3"],
        ],
    );
    wrk.create(
        "data2.csv",
        vec![
            svec!["city", "state", "pop"],
            svec!["Boston", "MA", "100"],
            svec!["Chicago", "IL", "300"],
            svec!["Miami", "FL", "400"],
        ],
    );

    let mut cmd = wrk.command("join");
    cmd.args(["city,state", "data1.csv", "city,state", "data2.csv"])
        .arg("--keys-output")
        .arg("keys.csv");

    wrk.run(&mut cmd);

    let got: Vec<Vec<String>> = wrk
        .read_to_string("keys.csv")
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split(',').map(String::from).collect())
        .collect();
    let expected = vec![svec!["Boston", "MA"], svec!["Chicago", "IL"]];
    assert_eq!(got, expected);
}

#[test]
fn join_keys_output_cross() {
    let wrk = Workdir::new("join_keys_cross");
    wrk.create("letters.csv", vec![svec!["letter"], svec!["a"], svec!["b"]]);
    wrk.create("numbers.csv", vec![svec!["num"], svec!["1"], svec!["2"]]);

    let mut cmd = wrk.command("join");
    cmd.args(["letter", "letters.csv", "num", "numbers.csv"])
        .arg("--cross")
        .arg("--keys-output")
        .arg("keys.csv");

    wrk.run(&mut cmd);

    // Cross join should not produce any keys output
    assert!(!wrk.path("keys.csv").exists());
}
