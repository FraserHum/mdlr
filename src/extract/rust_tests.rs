use super::*;
use std::path::PathBuf;

#[test]
fn test_extract_function() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
fn hello() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 2);
    assert_eq!(units[0].id, "test.rs::hello");
    assert_eq!(units[0].kind, UnitKind::Function);
    assert_eq!(units[1].id, "test.rs::add");
}

#[test]
fn test_extract_struct() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
struct Point {
    x: i32,
    y: i32,
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "test.rs::Point");
    assert_eq!(units[0].kind, UnitKind::Struct);
}

#[test]
fn test_extract_calls() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
fn caller() {
    foo();
    bar();
    baz::qux();
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 1);
    assert_eq!(units[0].calls, vec!["bar", "baz::qux", "foo"]);
}

#[test]
fn test_extract_module() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
mod inner {
    fn nested() {}
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "test.rs::inner::nested");
}

#[test]
fn test_extract_method_chains() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
fn chained() {
    obj.method();
    foo().bar();
    fs::write("path", "content").unwrap();
    TempDir::new().unwrap();
    some.long.chain().of().calls();
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 1);
    let calls = &units[0].calls;
    assert!(calls.contains(&"obj.method".to_string()));
    assert!(calls.contains(&"foo".to_string()));
    assert!(calls.contains(&"bar".to_string()));
    assert!(calls.contains(&"fs::write".to_string()));
    assert!(calls.contains(&"unwrap".to_string()));
    assert!(calls.contains(&"TempDir::new".to_string()));
    assert!(calls.contains(&"of".to_string()));
    assert!(calls.contains(&"calls".to_string()));
    // Should NOT contain the full multi-line expression
    assert!(!calls.iter().any(|c| c.contains("content")));
}

#[test]
fn test_extract_params() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
fn no_params() {}
fn one_param(a: i32) {}
fn two_params(a: i32, b: String) {}
fn with_self(&self, x: i32) {}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 4);
    assert_eq!(units[0].params, 0);
    assert_eq!(units[1].params, 1);
    assert_eq!(units[2].params, 2);
    assert_eq!(units[3].params, 1); // self doesn't count
}

#[test]
fn test_extract_branches() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
fn simple() {
    let x = 1;
}

fn with_if(x: i32) {
    if x > 0 {
        println!("positive");
    }
}

fn with_if_else(x: i32) {
    if x > 0 {
        println!("positive");
    } else {
        println!("non-positive");
    }
}

fn with_match(x: Option<i32>) {
    match x {
        Some(v) => println!("{}", v),
        None => println!("none"),
    }
}

fn with_loop() {
    for i in 0..10 {
        while true {
            break;
        }
    }
}

fn with_and_or(a: bool, b: bool, c: bool) {
    if a && b || c {
        println!("complex");
    }
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    assert_eq!(units.len(), 6);
    assert_eq!(units[0].branches, 0, "simple should have 0 branches");
    assert_eq!(units[1].branches, 1, "with_if should have 1 branch");
    assert_eq!(
        units[2].branches, 1,
        "with_if_else should have 1 branch (else doesn't add)"
    );
    assert_eq!(
        units[3].branches, 1,
        "with_match with 2 arms should have 1 branch"
    );
    assert_eq!(
        units[4].branches, 2,
        "with_loop should have 2 branches (for + while)"
    );
    assert_eq!(
        units[5].branches, 3,
        "with_and_or should have 3 branches (if + && + ||)"
    );
}

#[test]
fn test_extract_impl_methods() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
struct Foo {
    x: i32,
}

impl Foo {
    fn new() -> Self {
        Self { x: 0 }
    }

    fn get_x(&self) -> i32 {
        self.x
    }

    fn set_x(&mut self, val: i32) {
        self.x = val;
    }
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    // Should have: struct Foo, impl Foo, new, get_x, set_x
    assert_eq!(units.len(), 5);

    let impl_unit =
        units.iter().find(|u| u.id == "test.rs::impl Foo").unwrap();
    assert_eq!(impl_unit.kind, UnitKind::Impl);
    assert_eq!(impl_unit.impl_type, Some("Foo".to_string()));
    assert_eq!(impl_unit.impl_trait, None);

    let new_fn =
        units.iter().find(|u| u.id == "test.rs::impl Foo::new").unwrap();
    assert_eq!(new_fn.parent, Some("test.rs::impl Foo".to_string()));

    let get_x =
        units.iter().find(|u| u.id == "test.rs::impl Foo::get_x").unwrap();
    assert_eq!(get_x.parent, Some("test.rs::impl Foo".to_string()));
}

#[test]
fn test_extract_trait_impl() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
struct Bar;

impl Display for Bar {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "Bar")
    }
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    let impl_unit = units
        .iter()
        .find(|u| u.id == "test.rs::impl Display for Bar")
        .unwrap();
    assert_eq!(impl_unit.impl_type, Some("Bar".to_string()));
    assert_eq!(impl_unit.impl_trait, Some("Display".to_string()));

    let fmt_fn = units
        .iter()
        .find(|u| u.id == "test.rs::impl Display for Bar::fmt")
        .unwrap();
    assert_eq!(
        fmt_fn.parent,
        Some("test.rs::impl Display for Bar".to_string())
    );
}

#[test]
fn test_extract_field_access() {
    let extractor = RustExtractor::new().unwrap();
    let source = r#"
impl Foo {
    fn reader(&self) -> i32 {
        self.x + self.y
    }

    fn writer(&mut self) {
        self.x = 10;
        self.y = self.z;
    }
}
"#;
    let units =
        extractor.extract(source, &PathBuf::from("test.rs"), None).unwrap();

    let reader =
        units.iter().find(|u| u.id == "test.rs::impl Foo::reader").unwrap();
    assert!(reader.reads.contains(&"x".to_string()));
    assert!(reader.reads.contains(&"y".to_string()));
    assert!(reader.writes.is_empty());

    let writer =
        units.iter().find(|u| u.id == "test.rs::impl Foo::writer").unwrap();
    assert!(writer.writes.contains(&"x".to_string()));
    assert!(writer.writes.contains(&"y".to_string()));
    assert!(writer.reads.contains(&"z".to_string()));
}
