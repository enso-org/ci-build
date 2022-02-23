use crate::prelude::*;

use anyhow::Context;
use convert_case::Case;
use convert_case::Casing;
use proc_macro2::Ident;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use quote::ToTokens;
use std::collections::HashMap;
use std::collections::HashSet;
use syn::parse_quote;

lazy_static::lazy_static! {
    static ref PARAMETER: regex::Regex = regex::Regex::new(r"<([^>]+)>").unwrap();
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
    pub value: String,
    pub shape: Shape,
}

impl Node {
    pub fn new<'a>(text: &'a str) -> Node {
        let shape = Shape::new(&text);
        let value = text.trim_end_matches('/').into();
        Node { shape, value }
    }

    pub fn add_child(&mut self, node: Node) {
        println!("Adding {} to {}", node.value, self.value);
        match &mut self.shape {
            Shape::File => {
                self.shape = Shape::Directory(vec![node]);
            }
            Shape::Directory(dir) => {
                dir.push(node);
            }
        }
    }

    pub fn children(&self) -> &[Node] {
        match &self.shape {
            Shape::File => &[],
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

    pub fn path_formatter(&self, parameters_expr: impl ToTokens) -> TokenStream {
        let ret = PARAMETER.replace_all(&self.to_string(), "{}").to_string();
        let parameters = self.parameters().into_iter().map(|param| {
            let param = Ident::new(param, Span::call_site());
            quote! {
                #parameters_expr.#param.display()
            }
        });
        quote! {
            format!(#ret, #(#parameters),*)
        }
    }

    /// Collect parameter names used in this node's path segment.
    pub fn parameters(&self) -> Vec<&str> {
        PARAMETER
            .captures_iter(&self.value)
            .map(|captures| captures.get(1).unwrap().as_str())
            .collect()
    }

    /// Sanitize string to be a valid Rust identifier.
    pub fn rustify(&self) -> String {
        if self.value == "." {
            String::from("Paths")
        } else {
            let mut ret = self.value.replace(|c| matches!(c, '-' | '.' | ' '), "_");
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
    let parameters_var: Ident = parse_quote!(context);
    let ty_name = struct_ident(full_path.into_iter().cloned());
    let path_component = last_node.path_formatter(&parameters_var);

    let children_var = last_node.children().iter().map(Node::var_ident).collect_vec();
    let children_struct =
        last_node.children().iter().map(|child| child_struct_ident(full_path, child)).collect_vec();

    quote! {
       #[derive(Clone, Debug, Hash, PartialEq)]
       pub struct #ty_name {
           pub path: std::path::PathBuf,
           #(pub #children_var: #children_struct),*
       }

       impl #ty_name {
           #[allow(unused_variables)]
           pub fn new(#parameters_var: &Parameters, parent: &std::path::Path) -> Self {
               let path = parent.join(#path_component);
               #(let #children_var = #children_struct::new(#parameters_var, &path);)*
               Self { path, #(#children_var),* }
           }
       }

       impl AsRef<std::path::Path> for #ty_name {
           fn as_ref(&self) -> &std::path::Path {
               &self.path
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
    let variables: HashSet<&str> =
        forest.iter().flat_map(|tree| tree.iter().flat_map(|node| node.parameters())).collect();
    let variable_map = variables
        .iter()
        .map(|v| (*v, syn::Ident::new(v, Span::call_site())))
        .collect::<HashMap<_, _>>();

    // dbg!(&variables);
    let variable_idents = variable_map.values().collect_vec();
    let mut ret = quote! {
       #[derive(Clone, Debug, Hash, PartialEq)]
        pub struct Parameters {
            #(pub #variable_idents: std::path::PathBuf),*
        }

        // impl Parameters {
        //     pub fn new(#(#variable_idents: impl Into<std::path::PathBuf>),*) -> Self {
        //         Self {
        //             #(#variable_idents: #variable_idents.into()),*
        //         }
        //     }
        // }
    };

    let top = Node { value: String::from("."), shape: Shape::Directory(forest) };
    top.foreach(|full_path, last_node| {
        ret.extend(generate_struct(full_path, last_node));
    });
    Ok(ret)
}

pub fn convert(value: &serde_yaml::Value) -> Result<Vec<Node>> {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            let mut ret = Vec::new();
            for (key, value) in mapping {
                let segment = key.as_str().context(format!(
                    "Expected string with a path segment, found: {}",
                    serde_yaml::to_string(value)?
                ))?;

                let mut node = Node::new(segment);
                if !value.is_null() {
                    for child in convert(value)? {
                        node.add_child(child);
                    }
                }
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

    fn convert(value: &serde_yaml::Value) -> Result<Vec<Node>> {
        match value {
            serde_yaml::Value::Mapping(mapping) => {
                let mut ret = Vec::new();
                for (key, value) in mapping {
                    let segment = key.as_str().context(format!(
                        "Expected string with a path segment, found: {}",
                        serde_yaml::to_string(value)?
                    ))?;

                    let mut node = Node::new(segment);
                    if !value.is_null() {
                        for child in convert(value)? {
                            node.add_child(child);
                        }
                    }
                    ret.push(node)
                }
                Ok(ret)
            }
            _ => bail!("Expected YAML mapping, found the {}", serde_yaml::to_string(value)?),
        }
    }

    #[test]
    fn generate_paths() -> Result {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(YAML)?;
        let forest = convert(&yaml)?;
        let out = generate(forest)?;
        println!("===\n\n{}\n\n===", out);
        // dbg!(forest);
        Ok(())
    }
}
