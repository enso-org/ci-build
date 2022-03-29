use crate::prelude::*;

use anyhow::Context;
use convert_case::Case;
use convert_case::Casing;
use proc_macro2::Ident;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use regex::Regex;
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::iter::zip;


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

#[derive(Clone, Debug, PartialEq, Shrinkwrap)]
pub struct Node {
    #[shrinkwrap(main_field)]
    value:      String,
    parameters: BTreeSet<String>, // Wasteful but paths won't be that huge.
    /// The name that replaces value in variable-like contexts.
    /// Basically, we might not want use filepath name as name in the code.
    var_name:   Option<String>,
    shape:      Shape,
}

impl Node {
    pub fn new(value: impl AsRef<str>, var_name: Option<String>) -> Self {
        let shape = Shape::new(value.as_ref());
        let value = value.as_ref().trim_end_matches('/').to_string();
        let parameters = default();
        Self { var_name, parameters, shape, value }
    }

    pub fn new_from_key(value: &Value) -> Result<Self> {
        Ok(match value {
            Value::Mapping(mapping) => {
                let value = mapping[&"path".into()]
                    .as_str()
                    .context("Expected string for `path`")?
                    .to_owned();
                Node::new(value, mapping[&"var".into()].as_str().map(into))
            }
            Value::String(string) => Node::new(string, None),
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

    pub fn all_parameters_vars(&self) -> Vec<Ident> {
        self.parameters
            .iter()
            .sorted()
            .map(|name| syn::Ident::new(&name, Span::call_site()))
            .collect_vec()
    }

    pub fn own_parameters(&self) -> impl IntoIterator<Item = &str> {
        PARAMETER.find_all(&self.value)
    }

    pub fn own_parameter_vars(&self) -> Vec<Ident> {
        self.own_parameters().into_iter().map(to_ident).collect()
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

    pub fn foreach_<'a>(
        &'a self,
        stack: &mut Vec<&'a Node>,
        f: &mut impl FnMut(&[&'a Node], &'a Node),
    ) {
        stack.push(self);
        f(stack, self);
        if let Shape::Directory(children) = &self.shape {
            for child in children {
                child.foreach_(stack, f);
            }
        }
        assert_eq!(stack.pop(), Some(self));
    }

    pub fn foreach<'a>(&'a self, mut f: impl FnMut(&[&'a Node], &'a Node)) {
        let mut stack = Vec::new();
        self.foreach_(&mut stack, &mut f);
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

    /// Sanitize string to be a valid Rust identifier.
    pub fn rustify(&self) -> String {
        let base = self.var_name.as_ref().unwrap_or(&self.value);
        if base == "." {
            String::from("Paths")
        } else {
            let mut ret = base.replace(|c| matches!(c, '-' | '.' | ' '), "_");
            ret.remove_matches(|c| matches!(c, '<' | '>'));
            ret
        }
    }

    pub fn var_ident(&self) -> Ident {
        self.to_ident(Case::Snake)
    }

    pub fn struct_ident_piece(&self) -> Ident {
        self.to_ident(Case::Pascal)
    }

    pub fn to_ident(&self, case: Case) -> Ident {
        syn::Ident::new(&self.rustify().to_case(case), Span::call_site())
    }
}

pub fn struct_ident<'a>(full_path: impl IntoIterator<Item = &'a Node>) -> Ident {
    let text = full_path.into_iter().map(|n| n.struct_ident_piece()).join("");
    Ident::new(&text, Span::call_site())
}

pub fn child_struct_ident(init: &[&Node], last: &Node) -> Ident {
    struct_ident(init.iter().cloned().chain(once(last)))
}

pub fn generate_struct(full_path: &[&Node], last_node: &Node) -> TokenStream {
    let ty_name = struct_ident(full_path.into_iter().cloned());
    let path_component = last_node.path_formatter();

    let children_var = last_node.children().iter().map(Node::var_ident).collect_vec();
    let children_struct =
        last_node.children().iter().map(|child| child_struct_ident(full_path, child)).collect_vec();

    let parameter_vars = last_node.all_parameters_vars();
    let own_parameter_vars: Vec<Ident> = last_node.own_parameter_vars();

    let child_parameter_vars = last_node
        .children()
        .iter()
        .flat_map(|node| node.parameters.iter())
        .map(to_ident)
        .collect_vec();

    let children_init = zip(last_node.children(), &children_struct)
        .map(|(child, children_struct)| {
            let child_parameters = child.all_parameters_vars();
            quote! {
                #children_struct::new_under(&path, #(#child_parameters),*)
            }
        })
        .collect_vec();

    let opt_conversions = if parameter_vars.is_empty() {
        quote! {
            impl From<#ty_name> for std::path::PathBuf {
                fn from(value: #ty_name) -> Self {
                    value.path
                }
            }

            impl From<std::path::PathBuf> for #ty_name {
                fn from(value: std::path::PathBuf) -> Self {
                    #ty_name::new(value)
                }
            }

            impl From<&std::path::Path> for #ty_name {
                fn from(value: &std::path::Path) -> Self {
                    #ty_name::new(value)
                }
            }
        }
    } else {
        TokenStream::new()
    };

    quote! {
        #[derive(Clone, Debug, Hash, PartialEq)]
        pub struct #ty_name {
            pub path: std::path::PathBuf,
            #(pub #children_var: #children_struct),*
        }

        #opt_conversions

       impl #ty_name {
           pub fn new(path: impl Into<std::path::PathBuf> #(, #child_parameter_vars: impl AsRef<std::path::Path>)*) -> Self {
               let path = path.into();
               #(let #children_var = #children_init;)*
               Self { path, #(#children_var),* }
           }

           pub fn new_under(parent: impl AsRef<std::path::Path> #(, #parameter_vars: impl AsRef<std::path::Path>)*) -> Self {
               let path = parent.as_ref().join(Self::segment_name(#(#own_parameter_vars),*));
               Self::new(path, #(#child_parameter_vars),*)
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

       impl std::ops::Deref for #ty_name {
           type Target = std::path::PathBuf;
           fn deref(&self) -> &Self::Target {
               &self.path
           }
       }
    }
}

pub fn generate(forest: Vec<Node>) -> Result<proc_macro2::TokenStream> {
    let mut ret = TokenStream::new();
    for node in forest {
        node.foreach(|full_path, last_node| {
            ret.extend(generate_struct(full_path, last_node));
        })
    }
    Ok(ret)
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

    #[test]
    fn generate() -> Result {
        let yaml_contents = include_bytes!("../../build/ide-paths.yaml");
        let code = crate::paths::process(yaml_contents.as_slice())?;
        debug!("{}", code);
        Ok(())
    }
}
