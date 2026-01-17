# Installation

## From Source

```bash
git clone https://github.com/yourusername/mdlr.git
cd mdlr
cargo build --release
```

The binary will be at `target/release/mdlr`.

## Add to PATH

Option 1: Symlink to a directory in your PATH
```bash
ln -s $(pwd)/target/release/mdlr /usr/local/bin/mdlr
```

Option 2: Add the target directory to your PATH
```bash
export PATH="$PATH:/path/to/mdlr/target/release"
```
