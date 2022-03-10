use crate::prelude::*;

use regex::Regex;

pub struct Replacement {
    pattern:     Regex,
    replacement: String,
}

impl Replacement {
    pub fn new(pattern: impl AsRef<str>, replacement: impl Into<String>) -> Result<Self> {
        Ok(Self { pattern: Regex::new(pattern.as_ref())?, replacement: replacement.into() })
    }

    pub fn replace_all<'a>(&'_ self, text: &'a str) -> Cow<'a, str> {
        self.pattern.replace_all(text, &self.replacement)
    }
}

pub fn multi_replace_all<'a, 'b>(
    text: impl Into<String>,
    replacements: impl IntoIterator<Item = &'b Replacement>,
) -> String {
    let init = text.into();
    replacements
        .into_iter()
        .fold(init, |text, replacement| replacement.replace_all(&text).to_string())
}

/// Workaround fix by wdanilo, see: https://github.com/rustwasm/wasm-pack/issues/790
pub fn js_workaround_patcher(code: impl Into<String>) -> Result<String> {
    let replacements = vec![
        Replacement::new(r"(?s)if \(typeof input === 'string'.*return wasm;", "return imports")?,
        Replacement::new(
            r"(?s)if \(typeof input === 'undefined'.*const imports = \{\};",
            "const imports = {};",
        )?,
        Replacement::new(r"(?s)export default init;", "export default init")?,
    ];

    let patched_code = multi_replace_all(code, replacements.as_slice());
    let epilogue = r"export function after_load(w,m) { wasm = w; init.__wbindgen_wasm_module = m;}";
    Ok(format!("{patched_code}\n{epilogue}"))
}

pub fn patch_js_glue(code: impl Into<String>, output_path: impl AsRef<Path>) -> Result {
    println!("Patching {}.", output_path.as_ref().display());
    let patched_code = js_workaround_patcher(code)?;
    std::fs::write(output_path, patched_code)?;
    Ok(())
}
