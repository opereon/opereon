#![feature(async_closure)]

use tokio::prelude::*;
use tokio::runtime;

fn main() {
    let runtime = {
        let mut builder = runtime::Builder::new();
        builder.name_prefix("op-").build().unwrap()
    };
    
    runtime.block_on(async move {

    })
}