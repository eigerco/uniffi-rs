/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, fs::read_to_string, path::Path, str::FromStr};

use anyhow::Result;
use pulldown_cmark::{Event, HeadingLevel::H1, Parser, Tag};
use syn::Attribute;
use uniffi_meta::{AsType, Checksum};

/// Function documentation.
#[derive(Debug, Clone, PartialEq, Eq, Checksum)]
pub struct Function {
    pub description: String,
    pub arguments_descriptions: HashMap<String, String>,
    pub return_description: Option<String>,
}

impl FromStr for Function {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut description_buff = String::new();
        let mut args_values_buff: Vec<String> = Vec::new();
        let mut args_keys_buff: Vec<String> = Vec::new();

        let mut return_description_buff = String::new();

        let mut current_stage = ParseStage::Description;

        let parser = Parser::new(s);

        for event in parser {
            match event {
                Event::Start(Tag::Heading(H1, _, _)) => match current_stage {
                    ParseStage::Description => current_stage = ParseStage::Arguments,
                    ParseStage::Arguments => current_stage = ParseStage::ReturnDescription,
                    ParseStage::ReturnDescription => (),
                },
                Event::Text(s) => match current_stage {
                    ParseStage::Description => {
                        description_buff.push_str(&s);
                        description_buff.push('\n');
                    }
                    ParseStage::Arguments => {
                        if s.to_lowercase() == "arguments" {
                            continue;
                        }
                        args_values_buff.push(s.to_string());
                    }
                    ParseStage::ReturnDescription => {
                        if s.to_lowercase() == "returns" {
                            continue;
                        }
                        return_description_buff.push_str(&s);
                        return_description_buff.push('\n');
                    }
                },
                Event::Code(s) => {
                    args_keys_buff.push(s.to_string());
                }
                _ => (),
            }
        }

        let mut arguments_descriptions = HashMap::with_capacity(args_keys_buff.len());
        args_keys_buff
            .into_iter()
            .zip(args_values_buff.into_iter())
            .for_each(|(k, v)| {
                arguments_descriptions.insert(k, v.replace('-', "").trim().to_string());
            });

        let return_description = if return_description_buff.is_empty() {
            None
        } else {
            Some(return_description_buff)
        };

        if arguments_descriptions.is_empty() && return_description.is_none() {
            return Ok(Function {
                description: s.to_string(),
                arguments_descriptions,
                return_description,
            });
        }

        Ok(Function {
            description: description_buff,
            arguments_descriptions,
            return_description,
        })
    }
}

/// Used to keep track of the different
/// function comment parts while parsing it.
enum ParseStage {
    Description,
    Arguments,
    ReturnDescription,
}

/// Record or enum or object documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub description: String,

    /// Members (record fields or enum variants) descriptions.
    pub members: HashMap<String, String>,

    /// Methods documentation - empty for records and enums.
    pub methods: HashMap<String, Function>,
}

/// Impl documentation.
#[derive(Debug, PartialEq, Eq)]
struct Impl {
    methods: HashMap<String, Function>,
}

#[derive(Debug, PartialEq, Eq)]
struct Trait {
    /// The docs on the trait itself
    description: String,
    /// Methods documentation
    methods: HashMap<String, Function>,
}

// TODO(murph): is this even necessary? Is there overlap with normal structures
// or should I be creating a structure for the trait from the start
impl Into<Structure> for Trait {
    fn into(self) -> Structure {
        Structure {
            description: self.description,
            members: HashMap::default(),
            methods: self.methods,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Documentation {
    pub functions: HashMap<String, Function>,
    pub structures: HashMap<String, Structure>,
}

/// Extract doc comment from attributes.
///
/// Rust doc comments are silently converted (during parsing) to attributes of form:
/// #[doc = "documentation comment content"]
fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            attr.parse_meta().ok().and_then(|meta| {
                if let syn::Meta::NameValue(named_value) = meta {
                    let is_doc = named_value.path.is_ident("doc");
                    if is_doc {
                        match named_value.lit {
                            syn::Lit::Str(comment) => Some(comment.value().trim().to_string()),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

fn traverse_module_tree<P: AsRef<Path>>(path: P) -> Result<String> {
    let mut source_code_buff = String::new();

    let source_code = read_to_string(path.as_ref())?;
    let file = syn::parse_file(&source_code)?;

    source_code_buff.push_str(&source_code);

    for item in file.items.into_iter() {
        if let syn::Item::Mod(module) = item {
            let name = module.ident.to_string();

            let file_module = path.as_ref().with_file_name(format!("{name}.rs"));
            let to_traverse_further = if file_module.exists() {
                file_module
            } else {
                path.as_ref().with_file_name(format!("{name}/mod.rs"))
            };

            if to_traverse_further.exists() {
                source_code_buff.push_str(&traverse_module_tree(to_traverse_further)?)
            }
        }
    }

    Ok(source_code_buff)
}

/// Extract code documentation comments from `lib.rs` file contents.
pub fn extract_documentation(source_code: &str) -> Result<Documentation> {
    let file = syn::parse_file(source_code)?;

    let mut functions = HashMap::new();
    let mut structures = HashMap::new();
    let mut impls = HashMap::new();

    // we build traits up first so we know they're all there to be used when encountering impls later
    let mut traits: HashMap<String, Trait> = HashMap::new();

    // first pass to get trait documentation only
    for item in file.items.iter() {
        match item {
            syn::Item::Trait(item) => {
                if let Some(description) = extract_doc_comment(&item.attrs) {
                    let name = item.ident.to_string();
                    let methods = item
                        .items
                        .iter()
                        .filter_map(|item| {
                            if let syn::TraitItem::Method(method) = item {
                                let name = method.sig.ident.to_string();
                                extract_doc_comment(&method.attrs).map(|doc| (name, doc))
                            } else {
                                None
                            }
                        })
                        .map(|(name, description)| {
                            (name, Function::from_str(&description).unwrap())
                        })
                        .collect();

                    traits.insert(
                        name,
                        Trait {
                            description,
                            methods,
                        },
                    );
                }
            }
            _ => (), // other item types are ignored,
        }
    }

    for item in file.items.into_iter() {
        match item {
            syn::Item::Enum(item) => {
                if let Some(description) = extract_doc_comment(&item.attrs) {
                    let name = item.ident.to_string();

                    let members = item
                        .variants
                        .iter()
                        .filter_map(|variant| {
                            extract_doc_comment(&variant.attrs)
                                .map(|doc_comment| (variant.ident.to_string(), doc_comment))
                        })
                        .collect();

                    structures.insert(
                        name,
                        Structure {
                            description,
                            members,
                            methods: HashMap::default(),
                        },
                    );
                }
            }
            syn::Item::Struct(item) => {
                if let Some(description) = extract_doc_comment(&item.attrs) {
                    let name = item.ident.to_string();

                    let members = item
                        .fields
                        .iter()
                        .filter_map(|field| {
                            if let Some(ident) = &field.ident {
                                extract_doc_comment(&field.attrs)
                                    .map(|doc_comment| (ident.to_string(), doc_comment))
                            } else {
                                None
                            }
                        })
                        .collect();

                    structures.insert(
                        name,
                        Structure {
                            description,
                            members,
                            methods: HashMap::default(),
                        },
                    );
                }
            }
            syn::Item::Impl(item) => {
                if let syn::Type::Path(path) = *item.self_ty {
                    let name = path.path.segments[0].ident.to_string();
                    let maybe_trait_name = item.trait_.and_then(|t| match t {
                        (None, syn::Path { segments, .. }, _) => {
                            segments.first().map(|segment| segment.ident.to_string())
                        }
                        _ => None,
                    });
                    let methods: HashMap<String, Function> = item
                        .items
                        .into_iter()
                        .filter_map(|inner_item| {
                            if let syn::ImplItem::Method(method) = inner_item {
                                // if this is a trait impl, pull the doc from the trait for this method
                                // TODO(murph): right now the trait method comment shows up on CloakedAiInterface in Kotlin and nowhere in Python
                                // comments made directly on the impl for methods don't show up either
                                if let Some(trait_name) = &maybe_trait_name {
                                    let method_name = method.sig.ident.to_string();
                                    traits
                                        .get(trait_name)
                                        .and_then(|trait_doc| trait_doc.methods.get(&method_name))
                                        .map(|method_doc| {
                                            (method_name, method_doc.description.clone())
                                        })
                                } else {
                                    // if this isn't a trait impl (or there wasn't a doc for the trait method), get the
                                    // doc directly on the method
                                    let name = method.sig.ident.to_string();
                                    extract_doc_comment(&method.attrs).map(|doc| (name, doc))
                                }
                            } else {
                                None
                            }
                        })
                        .map(|(name, description)| {
                            (name, Function::from_str(&description).unwrap())
                        })
                        .collect();
                    impls
                        .entry(name)
                        .and_modify(|i: &mut Impl| 
                            // this is safe because impls can't have conflicting names for the same struct
                            i.methods.extend(methods.clone()))
                        .or_insert(Impl { methods });
                }
            }
            syn::Item::Fn(item) => {
                if let Some(description) = extract_doc_comment(&item.attrs) {
                    let name = item.sig.ident.to_string();
                    functions.insert(name, Function::from_str(&description).unwrap());
                }
            }
            _ => (), // other item types are ignored,
        }
    }

    for (name, impl_) in impls {
        if let Some(structure) = structures.get_mut(&name) {
            structure.methods = impl_.methods;
        }
    }

    // TODO(murph): this isn't being consumed how I thought it would. Check trait output in attached AST
    for (name, trait_) in traits {
            structures.insert(name, trait_.into());
    }

    Ok(Documentation {
        functions,
        structures,
    })
}

/// Extract code documentation comments from Rust `lib.rs` file.
pub fn extract_documentation_from_path<P: AsRef<Path>>(path: P) -> Result<Documentation> {
    let source_code = traverse_module_tree(path)?;
    extract_documentation(&source_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use quote::quote;

    #[test]
    fn test_doc_function_parses_a_md_description() {
        let description = indoc! {"
            This is the function description.
            Here is a second line.
            
            # Arguments
            
            - `argument1` - this is argument description 1.
            - `argument2` - this is argument description 2.
            
            # Returns
            
            This is return value description.
            Here is a second line.
        "};

        let result = Function::from_str(description).unwrap();
        assert_eq!(expected_complete_doc_function(), result);
    }

    fn expected_complete_doc_function() -> Function {
        let mut expected_arg_descriptions = HashMap::new();
        expected_arg_descriptions.insert(
            "argument1".to_string(),
            "this is argument description 1.".to_string(),
        );
        expected_arg_descriptions.insert(
            "argument2".to_string(),
            "this is argument description 2.".to_string(),
        );
        Function {
            description: "This is the function description.\nHere is a second line.\n".to_string(),
            arguments_descriptions: expected_arg_descriptions,
            return_description: Some(
                "This is return value description.\nHere is a second line.\n".to_string(),
            ),
        }
    }

    #[test]
    fn test_doc_function_parses_a_no_md_description() {
        let description = indoc! {"
            This is the function description.

            Arguments

            argument1 - this is argument description 1.
            argument2 - this is argument description 2.

            Returns

            This is return value description.
        "};

        let result = Function::from_str(description).unwrap();

        assert_eq!(
            Function {
                description: description.to_string(),
                arguments_descriptions: HashMap::new(),
                return_description: None
            },
            result
        );
    }

    #[test]
    fn test_extract_documentation() {
        let source_code = quote! {
            /// Person with a name.
            pub struct Person {
                inner: Mutex<simple::Person>,
            }

            impl Person {
                /// Create new person with [name].
                ///
                /// Example of multiline comment.
                pub fn new(name: String) -> Self {
                    Person {
                        inner: Mutex::new(simple::Person::new(&name)),
                    }
                }

                /// Set person name.
                pub fn set_name(&self, name: String) {
                    self.inner.lock().unwrap().set_name(&name);
                }

                /// Get person's name.
                ///
                /// Example of multiline comment.
                pub fn get_name(&self) -> String {
                    self.inner.lock().unwrap().get_name().to_string()
                }
            }

            impl Animal for Person {
                fn eat(&self, food: String) -> String {
                    format!("{} ate {food}.", self.get_name())
                }
            }

            /// Create hello message to a pet.
            ///
            /// # Arguments
            ///
            /// - `pet` - pet to create a message to.
            ///
            /// # Returns
            ///
            /// Hello message to a pet.
            pub fn hello(pet: Pet) -> String {
                simple::hello(pet.into())
            }

            /// Enum description.
            pub enum SomeEnum {
                /// A letter 'A'.
                A,

                /// A letter 'B'.
                B,

                /// A letter 'C'.
                C,
            }

            /// Functionality common to animals.
            pub trait Animal {
                /// Get a message about the Animal eating.
                fn eat(&self, food: String) -> String;
            }
        }
        .to_string();

        let documentation = extract_documentation(&source_code).unwrap();
        let mut structures = HashMap::new();

        let mut methods = HashMap::new();
        methods.insert(
            "new".to_string(),
            Function {
                description: indoc! {"
                Create new person with [name].
                
                Example of multiline comment.
            "}
                .trim()
                .to_string(),
                arguments_descriptions: HashMap::new(),
                return_description: None,
            },
        );
        methods.insert(
            "set_name".to_string(),
            Function {
                description: "Set person name.".to_string(),
                arguments_descriptions: HashMap::new(),
                return_description: None,
            },
        );
        methods.insert(
            "get_name".to_string(),
            Function {
                description: indoc! {"
                Get person's name.

                Example of multiline comment.
            "}
                .trim()
                .to_string(),
                arguments_descriptions: HashMap::new(),
                return_description: None,
            },
        );
        methods.insert(
            "eat".to_string(),
            Function {
                description: indoc! {"
                Get a message about the Animal eating.
            "}
                .trim()
                .to_string(),
                arguments_descriptions: HashMap::new(),
                return_description: None,
            },
        );

        structures.insert(
            "Person".to_string(),
            Structure {
                description: "Person with a name.".to_string(),
                members: HashMap::new(),
                methods,
            },
        );

        let mut members = HashMap::new();
        members.insert("A".to_string(), "A letter 'A'.".to_string());
        members.insert("B".to_string(), "A letter 'B'.".to_string());
        members.insert("C".to_string(), "A letter 'C'.".to_string());

        structures.insert(
            "SomeEnum".to_string(),
            Structure {
                description: "Enum description.".to_string(),
                members,
                methods: HashMap::new(),
            },
        );

        let mut arguments_descriptions = HashMap::new();
        arguments_descriptions.insert("pet".to_string(), "pet to create a message to.".to_string());

        let mut functions = HashMap::new();
        functions.insert(
            "hello".to_string(),
            Function {
                description: "Create hello message to a pet.\n".to_string(),
                arguments_descriptions,
                return_description: Some("Hello message to a pet.\n".to_string()),
            },
        );

        let mut methods = HashMap::new();
        methods.insert(
            "eat".to_string(),
            Function {
                description: indoc! {"
                Get a message about the Animal eating.
            "}
                .trim()
                .to_string(),
                arguments_descriptions: HashMap::new(),
                return_description: None,
            },
        );

        structures.insert(
            "Animal".to_string(),
            Structure {
                description: "Functionality common to animals.".to_string(),
                members: HashMap::new(),
                methods,
            },
        );

        let expected = Documentation {
            functions,
            structures,
        };

        assert_eq!(documentation, expected);
    }
}
