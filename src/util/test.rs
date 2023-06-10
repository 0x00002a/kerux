#[macro_export]
macro_rules! assert_ok {
    ($e:expr) => {
        if !$e.is_ok() {
            panic!(
                "assert_ok failed. expected ok but got: {err}",
                err = $e.unwrap_err()
            );
        }
    };
}
#[macro_export]
macro_rules! assert_err {
    ($e:expr) => {
        if !$e.is_err() {
            panic!("assert_err failed. expected err but got: {:?}", $e.unwrap());
        }
    };
}
