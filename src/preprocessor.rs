use mdbook::book::{Book, Chapter};
use mdbook::errors::Error;
use mdbook::errors::ErrorKind;
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::utils::fs::path_to_root;
use mdbook::utils::new_cmark_parser;
use mdbook::BookItem;
use pulldown_cmark as md;
use pulldown_cmark_to_cmark::fmt::cmark;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use toml::value::Table;
use toml::Value;

pub static PREPROCESSOR_NAME: &str = "tag";
pub static TAG_STRING_PREFIX: &str = "tag:";

pub struct TagPreprocessor {}

impl TagPreprocessor {
    pub fn new() -> TagPreprocessor {
        TagPreprocessor {}
    }
}

impl Preprocessor for TagPreprocessor {
    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let tagger = Tagger::new(ctx.config.get_preprocessor(self.name()));

        let mut tag_results: Vec<Result<Vec<AliasedTag>>> = vec![];

        book.for_each_mut(|item: &mut BookItem| {
            // only continue editing the book if we don't have any errors
            if !tag_results.iter().any(Result::is_err) {
                if let BookItem::Chapter(ref mut chapter) = item {
                    tag_results.push(tagger.process_chapter(chapter))
                }
            }
        });

        let tags = tag_results
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .map(|raw_tags| {
                let mut tags_map: HashMap<String, Vec<Tag>> = HashMap::new();

                // collect all of our tags
                for tag in raw_tags.into_iter().flatten() {
                    match tags_map.get_mut(&tag.alias) {
                        Some(existing_tags) => existing_tags.push(tag.tag),
                        None => {
                            tags_map.insert(tag.alias, vec![tag.tag]);
                        }
                    }
                }

                tags_map
            })?;

        if !tags.is_empty() {
            let tag_page = tagger.build_tags_page(tags)?;

            book.push_item(BookItem::Separator);
            book.push_item(tag_page);
        }

        Ok(book)
    }

    fn supports_renderer(&self, _renderer: &str) -> bool {
        // since we're just outputting markdown, this should support any renderer
        true
    }
}

struct Tagger {
    output_filename: String,
}

impl Tagger {
    pub fn new(config: Option<&Table>) -> Tagger {
        let output_filename: String = config
            .and_then(|t| t.get("filename"))
            .and_then(Value::as_str)
            .unwrap_or("tags.md")
            .into();

        Tagger { output_filename }
    }

    fn process_chapter(&self, chapter: &mut Chapter) -> Result<Vec<AliasedTag>> {
        let mut buf = String::with_capacity(chapter.content.len());
        let mut tags = vec![];

        let events = new_cmark_parser(&chapter.content).flat_map(|e| match e {
            md::Event::Code(ref raw_code) => {
                let code = raw_code.trim();

                if code.find(TAG_STRING_PREFIX) == Some(0) && code.len() > TAG_STRING_PREFIX.len() {
                    let alias = code[TAG_STRING_PREFIX.len()..].trim();

                    let tag = AliasedTag::new(
                        alias,
                        chapter.name.clone(),
                        chapter.path.clone(),
                        chapter.parent_names.clone(),
                    );

                    tags.push(tag);

                    let hash = format!("#{}", alias);
                    let link = md::Tag::Link(
                        md::LinkType::Inline,
                        format!(
                            "{}{}{}",
                            path_to_root(&chapter.path),
                            self.output_filename,
                            hash
                        )
                        .into(),
                        format!("Tag: {}", alias).into(),
                    );

                    vec![
                        md::Event::Start(link.clone()),
                        md::Event::Code(hash.into()),
                        md::Event::End(link),
                    ]
                } else {
                    vec![e]
                }
            }
            _ => vec![e],
        });

        cmark(events, &mut buf, None)
            .map_err(|err| Error::from(format!("Markdown serialization failed: {}", err)))?;

        chapter.content = buf;

        Ok(tags)
    }

    fn build_tags_page(&self, tags_map: HashMap<String, Vec<Tag>>) -> Result<Chapter> {
        let mut buf = String::new();

        let mut contents = vec![
            md::Event::Start(md::Tag::Header(1)),
            md::Event::Text("Tags".into()),
            md::Event::End(md::Tag::Header(1)),
        ];

        let mut sorted_tags = tags_map.into_iter().collect::<Vec<_>>();
        sorted_tags.sort_by(|a, b| a.0.cmp(&b.0));

        for (alias, mut tags) in sorted_tags {
            contents.push(md::Event::Start(md::Tag::Header(2)));
            contents.push(md::Event::Code(alias.into()));
            contents.push(md::Event::End(md::Tag::Header(2)));

            tags = {
                // order our tags by their paths
                let mut tags_sort_info = tags
                    .into_iter()
                    .map(|t| {
                        let mut sort_names = t.parent_names.clone();
                        sort_names.push(t.chapter_name.clone());

                        (t, sort_names)
                    })
                    .collect::<Vec<_>>();
                tags_sort_info.sort_by(|a, b| a.1.cmp(&b.1));

                tags_sort_info.into_iter().map(|t| t.0).collect()
            };

            for Tag {
                chapter_name,
                path,
                parent_names,
            } in tags.into_iter()
            {
                let parent_path: String = if !parent_names.is_empty() {
                    format!("/{}/", parent_names.join("/"))
                } else {
                    "/".into()
                };

                contents.push(md::Event::Text(parent_path.into()));

                let path_str: String = path
                    .to_str()
                    .ok_or_else(|| {
                        ErrorKind::Io(io::Error::new(
                            io::ErrorKind::NotFound,
                            "Couldn't build output path",
                        ))
                    })?
                    .into();

                let link = md::Tag::Link(
                    md::LinkType::Inline,
                    path_str.into(),
                    chapter_name.clone().into(),
                );

                contents.push(md::Event::Start(link.clone()));
                contents.push(md::Event::Text(chapter_name.into()));
                contents.push(md::Event::End(link.clone()));
                contents.push(md::Event::Text("\n\n".into()));
            }
        }

        cmark(contents.iter(), &mut buf, None)
            .map_err(|err| Error::from(format!("Markdown serialization failed: {}", err)))?;

        Ok(Chapter {
            name: "Tags".into(),
            content: buf,
            number: None,
            sub_items: vec![],
            path: format!("./{}", self.output_filename).into(),
            parent_names: vec![],
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct AliasedTag {
    alias: String,
    tag: Tag,
}

impl AliasedTag {
    fn new<S: Into<String>>(
        alias: S,
        chapter_name: String,
        path: PathBuf,
        parent_names: Vec<String>,
    ) -> AliasedTag {
        AliasedTag {
            alias: alias.into().to_ascii_lowercase(),
            tag: Tag {
                chapter_name,
                path,
                parent_names,
            },
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Tag {
    chapter_name: String,
    path: PathBuf,
    parent_names: Vec<String>,
}

#[cfg(test)]
mod test {
    use super::*;

    static CHAPTER_NAME: &str = "Test Chapter";
    static CHAPTER_FILE: &str = "chapter.md";

    mod process_chapter {
        use super::*;

        static EXPECTED: &str = r#"# Chapter

[`#hello`](tags.md#hello "Tag: hello")"#;

        #[test]
        fn simple_chapter() {
            let chapter = Chapter::new(
                CHAPTER_NAME,
                r#"# Chapter

`tag:hello`"#
                    .into(),
                PathBuf::from(format!("./{}", CHAPTER_FILE)),
                vec![],
            );

            verify_process_chapter(vec!["hello"], chapter, EXPECTED);
        }

        #[test]
        fn sub_dir_chapter() {
            let chapter = Chapter::new(
                CHAPTER_NAME,
                r#"# Chapter

`tag:hello`"#
                    .into(),
                PathBuf::from(format!("./subchapter/{}", CHAPTER_FILE)),
                vec![],
            );

            verify_process_chapter(
                vec!["hello"],
                chapter,
                r#"# Chapter

[`#hello`](../tags.md#hello "Tag: hello")"#,
            );
        }

        #[test]
        fn sub_chapter() {
            let chapter = Chapter::new(
                CHAPTER_NAME,
                r#"# Chapter

`tag:hello`"#
                    .into(),
                PathBuf::from(format!("./{}", CHAPTER_FILE)),
                vec!["Parent One".into(), "Parent Two".into()],
            );

            verify_process_chapter(vec!["hello"], chapter, EXPECTED);
        }

        fn verify_process_chapter(tag_aliases: Vec<&str>, mut chapter: Chapter, expected: &str) {
            let tagger = Tagger::new(None);
            let tags: Vec<_> = tag_aliases
                .into_iter()
                .map(|alias| {
                    AliasedTag::new(
                        alias,
                        chapter.name.clone(),
                        chapter.path.clone(),
                        chapter.parent_names.clone(),
                    )
                })
                .collect();

            assert_eq!(tags, tagger.process_chapter(&mut chapter).unwrap());

            assert_eq!(expected, chapter.content);
        }
    }

    mod build_tags_page {
        use super::*;
        use toml::map::Map;

        #[test]
        fn simple() {
            let tagger = Tagger::new(None);
            let mut tags: HashMap<String, _> = HashMap::new();
            tags.insert(
                "hello".into(),
                vec![Tag {
                    chapter_name: "Chapter".into(),
                    path: PathBuf::from("./chapter.md"),
                    parent_names: vec![],
                }],
            );
            let expected = r#"# Tags

## `hello`

/[Chapter](./chapter.md "Chapter")

"#;

            let chapter = tagger.build_tags_page(tags).unwrap();

            assert_eq!("Tags", chapter.name);
            assert_eq!(expected, chapter.content);
        }

        #[test]
        fn alternative_file_name() {
            let alternative_name = "my_tags.md";
            let mut config = Map::new();
            config.insert("filename".into(), Value::String(alternative_name.into()));

            let tagger = Tagger::new(Some(&config));

            let mut tags: HashMap<String, _> = HashMap::new();
            tags.insert(
                "hello".into(),
                vec![Tag {
                    chapter_name: "Chapter".into(),
                    path: PathBuf::from("./chapter.md"),
                    parent_names: vec![],
                }],
            );

            let chapter = tagger.build_tags_page(tags).unwrap();

            assert_eq!(
                PathBuf::from(format!("./{}", alternative_name)),
                chapter.path
            );
        }

        #[test]
        fn tag_sorting() {
            let tagger = Tagger::new(None);

            let chapter_tag = Tag {
                chapter_name: "Chapter".into(),
                path: PathBuf::from("./chapter.md"),
                parent_names: vec![],
            };
            let mut tags: HashMap<String, _> = HashMap::new();
            tags.insert("a".into(), vec![chapter_tag.clone()]);
            tags.insert("b".into(), vec![chapter_tag]);

            let expected = r#"# Tags

## `a`

/[Chapter](./chapter.md "Chapter")

## `b`

/[Chapter](./chapter.md "Chapter")

"#;

            let chapter = tagger.build_tags_page(tags).unwrap();

            assert_eq!(expected, chapter.content);
        }

        #[test]
        fn path_sorting() {
            let tagger = Tagger::new(None);

            let mut tags: HashMap<String, _> = HashMap::new();
            tags.insert(
                "a".into(),
                vec![
                    Tag {
                        chapter_name: "a".into(),
                        path: PathBuf::from("./chapter.md"),
                        parent_names: vec![],
                    },
                    Tag {
                        chapter_name: "a".into(),
                        path: PathBuf::from("./chapter.md"),
                        parent_names: vec!["a".into()],
                    },
                    Tag {
                        chapter_name: "b".into(),
                        path: PathBuf::from("./chapter.md"),
                        parent_names: vec!["b".into()],
                    },
                ],
            );

            let expected = r#"# Tags

## `a`

/[a](./chapter.md "a")

/a/[a](./chapter.md "a")

/b/[b](./chapter.md "b")

"#;

            let chapter = tagger.build_tags_page(tags).unwrap();

            assert_eq!(expected, chapter.content);
        }
    }
}
