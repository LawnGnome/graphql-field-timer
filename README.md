# GraphQL Field Timer

This tool takes a GraphQL query (and, optionally, the variables that might go
with it), and then issues a query for each single field defined in the query.
Once that's done, it shows you which are the slowest.

## Building

This is a pure Rust project, so `cargo build --release` should product a
`graphql-field-timer` binary in `target/release` pretty much regardless of
platform.

## Usage

_Very_ basic usage, assuming you have a query that doesn't require variables at
`query.graphql`:

```sh
graphql-field-timer -f query.graphql -u http://my.endpoint/graphql
```

If you need a header (for authorisation, most likely):

```sh
graphql-field-timer -f query.graphql -u http://my.endpoint/graphql --header 'Authorization: token foo'
```

If you have variables, you can provide them as a JSON blob, much as you would in
GraphiQL:

```sh
graphql-field-timer -f query.graphql -u http://my.endpoint/graphql -v '{"foo": "bar"}'
```
