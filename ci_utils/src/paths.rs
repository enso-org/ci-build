use crate::prelude::*;

use anyhow::Context;
use convert_case::Case;
use convert_case::Casing;
use proc_macro2::Ident;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use regex::Regex;
use std::collections::BTreeSet;
use std::iter::zip;

/// Sanitize string to be a valid Rust identifier.
fn normalize_ident(ident: impl AsRef<str>, case: Case) -> Ident {
    let base = ident.as_ref();
    let normalized_text = if base == "." {
        String::from("Paths")
    } else {
        let mut ret = base.replace(|c| matches!(c, '-' | '.' | ' '), "_");
        ret.remove_matches(|c| matches!(c, '<' | '>'));
        ret
    };

    Ident::new(&normalized_text.to_case(case), Span::call_site())
}

fn normalize_type_name(ident: impl AsRef<str>) -> Ident {
    normalize_ident(ident, Case::UpperCamel)
}

fn normalize_variable_name(ident: impl AsRef<str>) -> Ident {
    normalize_ident(ident, Case::Snake)
}

fn to_ident(name: impl AsRef<str>) -> Ident {
    syn::Ident::new(name.as_ref(), Span::call_site())
}

lazy_static::lazy_static! {
    /// Matches `bar` in `foo <bar> baz`.
    static ref PARAMETER: ParameterRegex = ParameterRegex::new();
}

#[derive(Clone, Debug, Shrinkwrap)]
pub struct ParameterRegex(Regex);

impl ParameterRegex {
    pub fn new() -> Self {
        Self(regex::Regex::new(r"<([^>]+)>").unwrap())
    }

    pub fn find_all<'a>(&'a self, text: &'a str) -> impl IntoIterator<Item = &str> {
        // The unwrap below is safe, if we have at least one explicit capture group in the regex.
        // We do have.
        self.0.captures_iter(text).map(|captures| captures.get(1).unwrap().as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    File,
    Directory(Vec<Node>),
}

impl Shape {
    pub fn new(text: impl AsRef<str>) -> Self {
        if text.as_ref().ends_with('/') {
            Shape::Directory(default())
        } else {
            Shape::File
        }
    }
}

pub fn get<'a>(
    mapping: &'a serde_yaml::Mapping,
    key: &(impl serde_yaml::mapping::Index + Display + ?Sized),
) -> Result<&'a serde_yaml::Value> {
    mapping.get(key).context(format!(
        "Missing key: {} in node {}",
        key,
        serde_yaml::to_string(mapping)?
    ))
}

pub fn get_string<'a>(
    mapping: &'a serde_yaml::Mapping,
    key: &(impl serde_yaml::mapping::Index + Display + ?Sized),
) -> Result<String> {
    get(mapping, key)?
        .as_str()
        .context(format!(
            "Expected string value for key: {} in node {}",
            key,
            serde_yaml::to_string(mapping)?
        ))
        .map(ToString::to_string)
}
#[derive(Debug)]
pub struct Generator<'a> {
    all_nodes: &'a [&'a Node],
    stack:     Vec<&'a Node>,
}

impl<'a> Generator<'a> {
    pub fn new(all_nodes: &'a [&'a Node]) -> Self {
        Self { all_nodes, stack: default() }
    }

    pub fn resolve(&self, r#type: &str) -> Result<&Node> {
        self.all_nodes
            .into_iter()
            .find(|node| node.matches_ref(r#type))
            .copied()
            .context(format!("Could not find node for type reference: {}", r#type))
    }

    pub fn generate(&mut self) -> Result<TokenStream> {
        let mut result = TokenStream::new();
        for node in self.all_nodes {
            result.extend(self.process_tree(node)?);
        }
        Ok(result)
    }

    pub fn process_tree(&mut self, node: &'a Node) -> Result<TokenStream> {
        self.stack.push(node);
        let mut result = self.generate_node(node)?;
        for child in node.children() {
            result.extend(self.process_tree(child)?);
        }
        let popped = self.stack.pop();
        ensure!(popped.is_some(), "Stack is empty");
        ensure!(popped.unwrap() == node, "Stack is corrupted");
        Ok(result)
    }

    pub fn generate_node(&mut self, last_node: &'a Node) -> Result<TokenStream> {
        if last_node.r#type.is_some() {
            // This node refers to another type, i.e. the structure we've already generated.
            return Ok(TokenStream::new());
        }

        let full_path = &self.stack;
        let ty_name = if let Some(r#type) = last_node.r#type.as_ref() {
            normalize_type_name(r#type)
        } else {
            struct_ident(full_path.into_iter().cloned())
        };
        let path_component = last_node.path_formatter();

        let children = last_node.children();
        let children_var = children.iter().map(Node::field_name).collect_vec();
        let children_struct =
            children.iter().map(|child| child.field_type(full_path)).collect_vec();

        let parameter_vars = last_node.all_parameters_vars();
        let own_parameter_vars = last_node.own_parameter_vars();
        let parent_parameter_vars: BTreeSet<_> =
            full_path.into_iter().flat_map(|n| n.own_parameter_vars()).collect();


        let child_parameter_vars: BTreeSet<_> = last_node
            .children()
            .iter()
            .flat_map(|node| node.parameters.iter())
            .map(to_ident)
            .collect();
        let all_parameters = {
            let mut v = parent_parameter_vars.clone();
            v.extend(child_parameter_vars.clone());
            v
        };

        let mut foo = vec![];
        for i in 0..full_path.len() {
            let nodes = &full_path[0..=i];
            let node = full_path[i];
            let ty_name = struct_ident(nodes.into_iter().cloned());
            let vars = node.own_parameter_vars();
            foo.push(quote! {
                #ty_name::segment_name(#(#vars),*)
            });
        }

        let children_init = zip(last_node.children(), &children_struct)
            .map(|(child, children_struct)| {
                if let Some(r#_type) = child.r#type.as_ref() {
                // FIXME this should resolve target type and use its parameters
                    let child_formatter = child.path_formatter();
                    let child_child_parameters = child.children_parameters();
                    quote! {
                        #children_struct::new_root(path.join(#child_formatter), #(&#child_child_parameters),*)
                    }
                } else {
                    let child_parameters = child.all_parameters_vars();
                    quote! {
                        #children_struct::new_under(&path, #(&#child_parameters),*)
                    }
                }
            })
            .collect_vec();

        let opt_conversions = if parameter_vars.is_empty() {
            quote! {
                impl From<std::path::PathBuf> for #ty_name {
                    fn from(value: std::path::PathBuf) -> Self {
                        #ty_name::new_root(value)
                    }
                }

                impl From<&std::path::Path> for #ty_name {
                    fn from(value: &std::path::Path) -> Self {
                        #ty_name::new_root(value)
                    }
                }
            }
        } else {
            TokenStream::new()
        };

        let ret = quote! {
            #[derive(Clone, Debug, Hash, PartialEq)]
            pub struct #ty_name {
                pub path: std::path::PathBuf,
                #(pub #children_var: #children_struct),*
            }

            #opt_conversions

           impl #ty_name {
               pub fn new(#(#all_parameters: impl AsRef<std::path::Path>, )*) -> Self {
                    let path = std::path::PathBuf::from_iter([#(#foo,)*]);
                    Self::new_root(path, #(#child_parameter_vars,)*)
               }

               pub fn new_root(path: impl Into<std::path::PathBuf> #(, #child_parameter_vars: impl AsRef<std::path::Path>)*) -> Self {
                   let path = path.into();
                   #(let #children_var = #children_init;)*
                   Self { path, #(#children_var),* }
               }

               pub fn new_under(parent: impl AsRef<std::path::Path> #(, #parameter_vars: impl AsRef<std::path::Path>)*) -> Self {
                   let path = parent.as_ref().join(Self::segment_name(#(#own_parameter_vars),*));
                   Self::new_root(path, #(#child_parameter_vars),*)
               }

                pub fn segment_name(#(#own_parameter_vars: impl AsRef<std::path::Path>),*) -> String {
                    #path_component
                }
           }

            impl std::fmt::Display for #ty_name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    self.path.display().fmt(f)
                }
            }

           impl AsRef<str> for #ty_name {
               fn as_ref(&self) -> &str {
                   &self.path.to_str().unwrap()
               }
           }

           impl AsRef<std::path::Path> for #ty_name {
               fn as_ref(&self) -> &std::path::Path {
                   &self.path
               }
           }

           impl AsRef<std::ffi::OsStr> for #ty_name {
               fn as_ref(&self) -> &std::ffi::OsStr {
                   self.path.as_ref()
               }
           }

            impl From<#ty_name> for std::path::PathBuf {
                fn from(value: #ty_name) -> Self {
                    value.path
                }
            }

           impl std::ops::Deref for #ty_name {
               type Target = std::path::PathBuf;
               fn deref(&self) -> &Self::Target {
                   &self.path
               }
           }
        };
        Ok(ret)
    }
}

#[derive(Clone, Debug, PartialEq, Shrinkwrap)]
pub struct Node {
    #[shrinkwrap(main_field)]
    value:      String,
    /// All parameters needed for this node (directly and for the children).
    parameters: BTreeSet<String>, // Wasteful but paths won't be that huge.
    /// The name that replaces value in variable-like contexts.
    /// Basically, we might not want use filepath name as name in the code.
    var_name:   Option<String>,
    shape:      Shape,
    r#type:     Option<String>,
}

impl Node {
    pub fn new(value: impl AsRef<str>) -> Self {
        let shape = Shape::new(value.as_ref());
        let value = value.as_ref().trim_end_matches('/').to_string();
        let parameters = default();
        let r#type = default();
        let var_name = default();
        Self { var_name, parameters, shape, value, r#type }
    }

    #[context("Failed to process node from key: {}", serde_yaml::to_string(value).unwrap())]
    pub fn new_from_key(value: &serde_yaml::Value) -> Result<Self> {
        Ok(match value {
            serde_yaml::Value::Mapping(mapping) => {
                let value = get_string(mapping, "path")?;
                let mut ret = Node::new(value);
                ret.var_name = get_string(mapping, "var").ok();
                ret.r#type = get_string(mapping, "type").ok();
                ret
            }
            serde_yaml::Value::String(string) => Node::new(string),
            other => bail!("Cannot deserialize {} to a node.", serde_yaml::to_string(other)?),
        })
    }

    pub fn add_child(&mut self, node: Node) {
        debug!("Adding {} to {}", node.value, self.value);
        match &mut self.shape {
            Shape::File => {
                self.shape = Shape::Directory(vec![node]);
            }
            Shape::Directory(dir) => {
                dir.push(node);
            }
        }
    }

    pub fn all_parameters_vars(&self) -> BTreeSet<Ident> {
        self.parameters.iter().sorted().map(to_ident).collect()
    }

    pub fn own_parameters(&self) -> impl IntoIterator<Item = &str> {
        PARAMETER.find_all(&self.value)
    }

    pub fn own_parameter_vars(&self) -> BTreeSet<Ident> {
        self.own_parameters().into_iter().map(to_ident).collect()
    }

    pub fn children_parameters(&self) -> BTreeSet<Ident> {
        self.children()
            .into_iter()
            .flat_map(|child| child.parameters.iter())
            .map(to_ident)
            .collect()
    }

    pub fn children(&self) -> &[Node] {
        match &self.shape {
            Shape::File => &[],
            Shape::Directory(dir) => dir,
        }
    }

    pub fn children_mut(&mut self) -> &mut [Node] {
        match &mut self.shape {
            Shape::File => &mut [],
            Shape::Directory(dir) => dir,
        }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a Self> + 'a {
        let me = once(self);
        let children = self.children().iter().flat_map(|child| child.iter());
        // Must be boxed because it would leak a recursive lambda type otherwise.
        type Erased<'a> = Box<dyn Iterator<Item = &'a Node> + 'a>;
        let children = Box::new(children) as Erased;
        me.chain(children)
    }

    pub fn path_formatter(&self) -> TokenStream {
        let text = self.to_string();
        let ret = PARAMETER.replace_all(&text, "{}").to_string();
        let parameters = self.own_parameters().into_iter().map(|param| {
            let param = Ident::new(param, Span::call_site());
            quote! {
                #param.as_ref().display()
            }
        });
        quote! {
            format!(#ret, #(#parameters),*)
        }
    }

    pub fn matches_ref(&self, r#type: &str) -> bool {
        self.var_name.as_ref().contains(&r#type)
    }

    pub fn field_name(&self) -> Ident {
        normalize_variable_name(self.var_name.as_ref().unwrap_or(&self.value))
    }

    pub fn field_type(&self, init: &[&Node]) -> Ident {
        if let Some(r#type) = &self.r#type {
            normalize_type_name(r#type)
        } else {
            struct_ident(init.into_iter().cloned().chain(once(self)))
        }
    }

    pub fn struct_ident_piece(&self) -> Ident {
        normalize_type_name(self.var_name.as_ref().unwrap_or(&self.value))
    }
}

pub fn struct_ident<'a>(full_path: impl IntoIterator<Item = &'a Node>) -> Ident {
    let text = full_path.into_iter().map(|n| n.struct_ident_piece()).join("");
    Ident::new(&text, Span::call_site())
}

pub fn child_struct_ident(init: &[&Node], last: &Node) -> Ident {
    struct_ident(init.iter().cloned().chain(once(last)))
}

pub fn generate(forest: Vec<Node>) -> Result<TokenStream> {
    let all_node_refs = forest.iter().collect_vec();
    let mut generator = Generator::new(&all_node_refs);
    generator.generate()
}

pub fn collect_parameters(value: &mut Node) {
    let mut child_parameters = BTreeSet::new();
    for child in value.children_mut() {
        collect_parameters(child);
        child_parameters.extend(child.parameters.clone());
    }

    let own_parameters = PARAMETER.find_all(&value.value).into_iter().map(ToString::to_string);
    value.parameters.extend(own_parameters);
    value.parameters.extend(child_parameters);
    debug!("{} has {} parameters", value.value, value.parameters.len());
}

pub fn convert(value: &serde_yaml::Value) -> Result<Vec<Node>> {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            let mut ret = Vec::new();
            for (key, value) in mapping {
                let mut node = Node::new_from_key(key)?;
                if !value.is_null() {
                    for child in convert(value)? {
                        node.add_child(child);
                    }
                }
                collect_parameters(&mut node);
                ret.push(node)
            }
            Ok(ret)
        }
        _ => bail!("Expected YAML mapping, found the {}", serde_yaml::to_string(value)?),
    }
}

pub fn process(yaml_input: impl Read) -> Result<String> {
    let yaml = serde_yaml::from_reader(yaml_input)?;
    let forest = convert(&yaml)?;
    let out = generate(forest)?;
    Ok(out.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::setup_logging;

    #[test]
    #[ignore]
    fn generate() -> Result {
        setup_logging()?;
        let yaml_contents = include_bytes!("../../build/ide-paths.yaml");
        let code = process(yaml_contents.as_slice())?;
        debug!("{}", code);
        Ok(())
    }
}
