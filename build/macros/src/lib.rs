// #![feature(default_free_fn)]
// #![feature(generators)]
// #![feature(type_alias_impl_trait)]
// #![feature(string_remove_matches)]
//
// const YAML: &'static str = r#"
// <repo_root>/:
//     .github/:
//         workflows/:
//     app/:
//         gui/:
//         ide-desktop/:
//             lib/:
//                 content/:
//                 project-manager/:
//     build/:
//     dist/:
//         bin/:
//         client/:
//         content/:
//             assets/:
//             package.json:
//             preload.js:
//         tmp/:
//         // Final WASM artifacts in `dist` directory.:
//         wasm/:
//             <WASM_MAIN>:
//             <WASM_MAIN_RAW>:
//             <WASM_GLUE>:
//         init:
//         build-init:
//         build.json:
//     run:
// <os_temp>:
//     enso-wasm/:
//         <WASM_MAIN>:
//         <WASM_MAIN_RAW>:
//         ide.wasm.gz:
// "#;
//
//
// mod prelude {
//     pub use ide_ci::prelude::*;
// }
//
// use crate::prelude::*;
// use crate::Shape::Directory;
// use convert_case::Case;
// use convert_case::Casing;
// use proc_macro2::Ident;
// use proc_macro2::Span;
// use proc_macro2::TokenStream;
// use quote::quote;
// use quote::ToTokens;
// use std::collections::HashMap;
// use std::collections::HashSet;
// use syn::parse_quote;
// use syn::LitStr;
//
// lazy_static::lazy_static! {
//     static ref PARAMETER: regex::Regex = regex::Regex::new(r"<([^>]+)>").unwrap();
// }
//
//
// #[derive(Clone, Debug, PartialEq)]
// enum Shape<T> {
//     File,
//     Directory(Vec<Node<T>>),
// }
//
// impl<T> Shape<T> {
//     pub fn new(text: impl AsRef<str>) -> Self {
//         if text.as_ref().ends_with('/') {
//             Shape::Directory(default())
//         } else {
//             Shape::File
//         }
//     }
// }
//
// #[derive(Clone, Debug, PartialEq)]
// enum Value {
//     Literal(PathBuf),
//     Interpolated(String),
// }
//
// impl Display for Value {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", match self {
//             Value::Literal(path) => path.as_str(),
//             Value::Interpolated(text) => text,
//         })
//     }
// }
//
// impl From<&str> for Value {
//     fn from(text: &str) -> Self {
//         if PARAMETER.is_match(text) {
//             Value::Interpolated(text.to_string())
//         } else {
//             Value::Literal(text.into())
//         }
//     }
// }
//
// impl Value {
//     fn path_formatter(&self, parameters_expr: impl ToTokens) -> TokenStream {
//         let ret = PARAMETER.replace_all(&self.to_string(), "{}").to_string();
//         let parameters = self.parameters().into_iter().map(|param| {
//             let param = Ident::new(param, Span::call_site());
//             quote! {
//                 #parameters_expr.#param.display()
//             }
//         });
//         quote! {
//             format!(#ret, #(#parameters),*)
//         }
//     }
//
//     fn parameters(&self) -> Vec<&str> {
//         match self {
//             Value::Literal(_) => default(),
//             Value::Interpolated(text) => PARAMETER
//                 .captures_iter(text)
//                 .map(|captures| captures.get(1).unwrap().as_str())
//                 .collect(),
//         }
//     }
//
//     /// Sanitize string to be a valid Rust identifier.
//     fn rustify(&self) -> String {
//         let mut ret = self.to_string();
//         ret.remove_matches(|c| c == '<' || c == '>');
//         ret = ret.replace(|c| c == '-' || c == '.' || c == ' ', "_");
//         ret
//     }
//
//
//     fn var_ident(&self) -> Ident {
//         syn::Ident::new(&self.rustify().to_case(Case::Snake), Span::call_site())
//     }
//
//     fn struct_ident_piece(&self) -> Ident {
//         syn::Ident::new(&self.rustify().to_case(Case::Pascal), Span::call_site())
//     }
// }
//
// #[derive(Clone, Debug, PartialEq, Shrinkwrap)]
// struct Node<T> {
//     #[shrinkwrap(main_field)]
//     pub value: T,
//     pub shape: Shape<T>,
// }
//
// impl<T> Node<T> {
//     fn new<'a>(text: &'a str) -> Node<T>
//     where T: From<&'a str> {
//         let shape = Shape::new(&text);
//         let value = text.trim_end_matches('/').into();
//         Node { shape, value }
//     }
//
//     fn add_child(&mut self, node: Node<T>)
//     where T: Display {
//         println!("Adding {} to {}", node.value, self.value);
//         match &mut self.shape {
//             Shape::File => {
//                 self.shape = Shape::Directory(vec![node]);
//             }
//             Directory(dir) => {
//                 dir.push(node);
//             }
//         }
//     }
//
//     fn children(&self) -> &[Node<T>] {
//         match &self.shape {
//             Shape::File => &[],
//             Directory(dir) => dir,
//         }
//     }
//
//     fn foreach_<'a>(
//         &'a self,
//         stack: &mut Vec<&'a Node<T>>,
//         f: &mut impl FnMut(&[&'a Node<T>], &'a Node<T>),
//     ) where
//         T: Debug + PartialEq,
//     {
//         stack.push(self);
//         f(stack, self);
//         if let Shape::Directory(children) = &self.shape {
//             for child in children {
//                 child.foreach_(stack, f);
//             }
//         }
//         assert_eq!(stack.pop(), Some(self));
//     }
//
//     fn foreach<'a>(&'a self, mut f: impl FnMut(&[&'a Node<T>], &'a Node<T>))
//     where T: Debug + PartialEq {
//         let mut stack = Vec::new();
//         self.foreach_(&mut stack, &mut f);
//     }
//
//     fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &'a Self> + 'a>
//     where T: Debug + PartialEq {
//         let me = once(self);
//         let children = self.children().iter().flat_map(|child| child.iter());
//         Box::new(me.chain(children))
//     }
// }
//
// fn parse(input: &str) -> Result<Vec<Node<Value>>> {
//     let input = input.replace("\t", "    ");
//
//     let mut ret = Vec::<Node<Value>>::new();
//
//     let mut stack: Vec<(usize, Node<Value>)> = Vec::new();
//
//     let entries = input
//         .lines()
//         .filter(|line| {
//             let trimmed_line = line.trim();
//             let is_comment = trimmed_line.starts_with("//");
//             !trimmed_line.is_empty() && !is_comment
//         })
//         .map(|line| {
//             let first_non_white = line.find(|c: char| !c.is_whitespace());
//             let (indent, tail) = line.split_at(first_non_white.unwrap_or(0));
//             (indent.len(), tail)
//         });
//
//
//
//     for (indent, text) in entries {
//         println!("Processing {indent} => {text}");
//
//         let node = Node::new(text);
//
//         // Close any entries that are not our parent.
//         while let Some((previous_indent, previous)) = stack.pop() {
//             if previous_indent < indent {
//                 stack.push((previous_indent, previous));
//                 break;
//             } else if let Some(previous_previous) = stack.last_mut() {
//                 previous_previous.1.add_child(previous);
//             } else {
//                 ret.push(previous);
//             }
//         }
//
//
//         match stack.last_mut() {
//             Some((previous_indent, _previous)) =>
//                 if indent > *previous_indent {
//                     stack.push((indent, node));
//                 } else {
//                 },
//             None => {
//                 stack.push((indent, node));
//             }
//         };
//     }
//
//     while let Some((_previous_indent, previous)) = stack.pop() {
//         if let Some(previous_previous) = stack.last_mut() {
//             previous_previous.1.add_child(previous);
//         } else {
//             ret.push(previous);
//         }
//     }
//
//     Ok(ret)
// }
//
// fn struct_ident<'a, T: Borrow<Value> + 'a>(full_path: impl IntoIterator<Item = &'a T>) -> Ident {
//     let text = full_path.into_iter().map(|n| n.borrow().struct_ident_piece()).join("");
//     Ident::new(&text, Span::call_site())
// }
//
// fn struct_ident_(init: &[&Node<Value>], last: &Node<Value>) -> Ident {
//     struct_ident(init.iter().cloned().chain(once(last)))
// }
//
// fn generate(forest: Vec<Node<Value>>) -> Result<proc_macro2::TokenStream> {
//     let variables: HashSet<&str> = forest
//         .iter()
//         .flat_map(|tree| tree.iter().flat_map(|node| node.value.parameters()))
//         .collect();
//     let variable_map = variables
//         .iter()
//         .map(|v| (*v, syn::Ident::new(v, Span::call_site())))
//         .collect::<HashMap<_, _>>();
//
//     // dbg!(&variables);
//     let variable_idents = variable_map.values().collect_vec();
//
//     let mut ret = quote! {
//         pub struct Parameters {
//             pub #(#variable_idents: std::path::PathBuf),*
//         }
//
//         impl Parameters {
//             pub fn new(#(#variable_idents: impl Into<std::path::PathBuf>),*) -> Self {
//                 Self {
//                     #(#variable_idents: #variable_idents.into()),*
//                 }
//             }
//         }
//     };
//
//     forest.first().unwrap().foreach(|full_path, last_node| {
//         let parameters: Ident = parse_quote!(context);
//         let ty_name = struct_ident(full_path.into_iter().cloned());
//         let path_component = last_node.path_formatter(&parameters);
//
//         let children_var = last_node.children().iter().map(|child|
// child.var_ident()).collect_vec();         let children_struct =
//             last_node.children().iter().map(|child| struct_ident_(full_path,
// child)).collect_vec();
//
//         ret.extend(quote! {
//             pub struct #ty_name {
//                 pub path: std::path::PathBuf,
//                 #(pub #children_var: #children_struct),*
//             }
//
//             impl #ty_name {
//                 pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//                     let path = parent.join(#path_component);
//                     #(let #children_var = #children_struct::new(context, &path);)*
//                     Self { path, #(#children_var),* }
//                 }
//             }
//
//             impl AsRef<std::path::Path> for #ty_name {
//                 fn as_ref(&self) -> &std::path::Path {
//                     &self.path
//                 }
//             }
//
//             impl std::ops::Deref for #ty_name {
//                 type Target = std::path::PathBuf;
//                 fn deref(&self) -> &Self::Target {
//                     &self.path
//                 }
//             }
//         })
//     });
//
//
//     Ok(ret)
// }
//
// #[proc_macro]
// pub fn make_answer(_item: proc_macro::TokenStream) -> proc_macro::TokenStream {
//     let string = syn::parse::<LitStr>(_item).unwrap();
//     let tree = parse(&string.value()).unwrap();
//     let out = generate(tree).unwrap();
//     println!("{}", out);
//     out.into()
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn parse_test() -> Result {
//         let yaml = serde_
//         // let tree = dbg!(parse(PATHS))?;
//         // let out = generate(tree)?;
//         // println!("{out}");
//         Ok(())
//     }
// }
