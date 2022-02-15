use crate::prelude::*;

const TREE: &str = r#"
enso-1.0.0
├── manifest.yaml    # A manifest file defining metadata about this Enso version.
├── component        # Contains all the executable tools and their dependencies.
│   ├── runner.jar   # The main executable of the distribution. CLI entry point.
│   └── runtime.jar  # The language runtime. It is loaded by other JVM components, like the runner.
├── native-libraries # Contains all shared libraries that are used by JVM components.
│   └── parser.so    # The language parser. It is loaded by the runtime component.
│                    # Alternative extensions are .dll Windows and .dylib on Mac.
└── lib          # Contains all the libraries that are pre-installed within that compiler version.
    └── Standard
        ├── Http
        │   └── 0.1.0     # Every version sub-directory is just an Enso package containing the library.
        │       ├── package.yaml
        │       ├── polyglot
        │       └── src
        │           ├── Http.enso
        │           └── Socket.enso
        └── Base
            └── 0.1.0
                ├── package.yaml
                └── src
                    ├── List.enso
                    ├── Number.enso
                    └── Text.enso
"#;

pub enum Shape<T> {
    File,
    Directory(Vec<Node<T>>),
}

pub struct Node<T> {
    pub value: T,
    pub shape: Shape<T>,
}
