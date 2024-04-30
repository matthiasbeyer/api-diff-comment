# api-diff-comment

Generate a `cargo-public-api` like diff of two public APIs and render them with
a template.

## How it works

This tool creates new (temporary) worktree checkouts of a git repository, diffs
both (as in `cargo-public-api diff`) and renders the diff in a template the user
provides.

The template is rendered with handlebars syntax, the variables are:

**TODO**

## License

GPL-2.0 only

(c) 2024 Matthias Beyer
