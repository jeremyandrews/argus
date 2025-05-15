#!/bin/bash

# Shell script wrapper for the alias management tool
# This makes it more convenient to run the various alias management commands

# Pass all arguments to the Rust binary
cargo run --bin manage_aliases -- "$@"
