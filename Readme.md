# mdBook Tag

[![crates.io](https://img.shields.io/crates/v/mdbook-tag.svg)](https://crates.io/crates/mdbook-tag)
[![LICENSE](https://img.shields.io/github/license/dylanowen/mdbook-tag.svg)](LICENSE)

## Install

```
cargo install mdbook-tag
```

## Usage

`book.toml`
```toml
[preprocessor.tag]
command = "mdbook-tag"
# Optional key to customize the output filename (defaults to tags.md)
filename = "customtagsfile.md"
```

#### Input

~~~markdown
`tag:one-tag` `tag:two-tag`
~~~

#### Output

~~~markdown
[`#one-tag`](/tags.md#one-tag "Tag: one-tag") [`#two-tag`](/tags.md#two-tag "Tag: two-tag")
~~~

#### Rendered

[`#one-tag`](/tags.md#one-tag "Tag: one-tag") [`#two-tag`](/tags.md#two-tag "Tag: two-tag")