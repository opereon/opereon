# opereon
[![Build Status](https://travis-ci.org/opereon/opereon.svg?branch=master)](https://travis-ci.org/opereon/opereon)
[![codecov](https://codecov.io/gh/opereon/opereon/branch/master/graph/badge.svg)](https://codecov.io/gh/opereon/opereon)

https://opereon.io/

This crate contains code from crates:

* https://github.com/kodegenix/kg-diag/tree/master/kg-diag-derive
* https://github.com/kodegenix/kg-diag/tree/master/kg-diag
* https://github.com/kodegenix/kg-display/tree/master/kg-display-derive
* https://github.com/kodegenix/kg-display/tree/master/kg-display
* https://github.com/kodegenix/kg-js
* https://github.com/kodegenix/kg-lang
* https://github.com/kodegenix/kg-symbol
* https://github.com/kodegenix/kg-template
* https://github.com/kodegenix/kg-tree
* https://github.com/kodegenix/kg-utils

## Builds statuses for Rust channels

| stable            | beta              | nightly           |
|-------------------|-------------------|-------------------|
| [![Build1][3]][4] | [![Build2][2]][4] | [![Build3][1]][4] |

[1]: https://travis-matrix-badges.herokuapp.com/repos/opereon/opereon/branches/master/1
[2]: https://travis-matrix-badges.herokuapp.com/repos/opereon/opereon/branches/master/2
[3]: https://travis-matrix-badges.herokuapp.com/repos/opereon/opereon/branches/master/3
[4]: https://travis-ci.org/opereon/opereon

## Opereon example environment
Example model is located in `op-cli/tests/resources/model`.
To configure opereon example environment:

- add `op` executable to `$PATH`, for example in `.bashrc` add `export PATH="[path_to_opereon_repository]/target/debug:$PATH"`
- `chmod 600 resources/model/keys/vagrant` - change permissions of private keys used by hosts
- `cd resources/model && op init && op commit` - initialize model git repository and make initial model commit
- install `docker` and `docker-compose`
- execute `./restart-env` to rebuild from scratch and start example hosts.
- execute `./ssh-into [host] [command]` to connect/execute command via ssh on example hosts. Available hosts - `ares`, `zeus`.
- execute `./stop-env` to stop and remove example hosts
- **Important!** Before first use of `op` you have to manually accept example hosts fingerprints. 
To do this simply execute `./ssh-into [host]` on each hosts and accept fingerprints.
Hosts are configured to preserve their fingerprints so there is no need to repeat this step in normal circumstances.

## Opereon system tests
Opereon system tests are implemented as Rust integration tests in `op-cli` crate. 
They are available when compiled with `system-tests` feature.
Multi-host environment is simulated by docker containers used also in example environment.
Each test creates full opereon environment - model dir, and temporary hosts.
Tests are executed with use of latest `op` executable compilation located in `target/debug/op`.

To run system tests:

- `chmod 600 op-cli/tests/resources/model/keys/vagrant` - change permissions of private keys used by hosts
- install `docker` and `docker-compose`
- **Important!** Before running the tests for the first time you have to manually accept test hosts fingerprints.
To do this simply execute `./ssh-into [host]` on each hosts and accept fingerprints as described in previous section.
- **Important!** Before running tests make sure that example environment described in previous section is down (script `./stop-env`).
This is necessary because example and test hosts binds same ssh ports.
- execute `./system-tests.sh` located in project root to start tests.

## License

Licensed under either of
* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## Copyright

Copyright (c) 2018 Kodegenix Sp. z o.o. [http://www.kodegenix.pl](http://www.kodegenix.pl)
