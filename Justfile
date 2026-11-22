# Check the crate and the iOS app against tests/cases.json
test: rust-test ios-test

# The crate's tests, including tests/cases.json, and clippy
rust-test:
    cargo test && cargo clippy --all-targets

# The iOS app's core, run against tests/cases.json on Linux
ios-test:
    swiftc -parse-as-library ios/Sources/Core.swift -o /tmp/refdir-ios-test && /tmp/refdir-ios-test
    swiftc -parse-as-library -typecheck ios/Sources/Core.swift ios/Sources/Fetch.swift
