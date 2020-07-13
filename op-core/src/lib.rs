#![feature(specialization)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_diag_derive;

#[macro_use]
extern crate kg_display_derive;

mod ops;
mod utils;
pub mod config;
pub mod outcome;
pub mod context;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
