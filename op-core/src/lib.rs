#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

mod ops;
mod outcome;
mod utils;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
