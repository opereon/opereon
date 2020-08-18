//#![deny(warnings)]
#[macro_use]
extern crate pretty_assertions;

#[macro_use]
extern crate op_test_helpers;

macro_rules! aw {
    ($e:expr) => {
        tokio_test::block_on($e)
    };
}

mod tests;
