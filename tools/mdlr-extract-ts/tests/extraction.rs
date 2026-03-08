use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct FileCacheEntry {
    units: Vec<Unit>,
}

#[derive(Debug, Deserialize)]
struct Unit {
    id: String,
    kind: String,
    reads: Vec<String>,
    writes: Vec<String>,
    calls: Vec<String>,
    params: usize,
    branches: usize,
    max_scope_lines: usize,
    #[serde(default)]
    parent: Option<String>,
}

/// Run the extractor on a temp directory with a single TS file and return units keyed by id.
fn extract(source: &str) -> HashMap<String, Unit> {
    extract_file("src/test.ts", source)
}

fn extract_file(rel_path: &str, source: &str) -> HashMap<String, Unit> {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let root = tmp.path();

    let file_path = root.join(rel_path);
    std::fs::create_dir_all(file_path.parent().unwrap()).expect("mkdir");
    std::fs::write(&file_path, source).expect("write source");

    let extractor = find_extractor();
    let output_dir = root.join("output");
    std::fs::create_dir_all(&output_dir).expect("mkdir output");

    let status = Command::new(&extractor)
        .arg("--root")
        .arg(root)
        .arg("--output")
        .arg(&output_dir)
        .arg("--generation-id")
        .arg("1")
        .status()
        .expect("run extractor");

    assert!(status.success(), "extractor exited with {status}");

    let json_files = find_json_files(&output_dir);
    assert!(
        !json_files.is_empty(),
        "no JSON output files in {}",
        output_dir.display()
    );

    let mut units = HashMap::new();
    for json_file in &json_files {
        let content = std::fs::read_to_string(json_file)
            .unwrap_or_else(|e| panic!("read {}: {e}", json_file.display()));
        let entry: FileCacheEntry = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("parse {}: {e}", json_file.display()));
        for unit in entry.units {
            units.insert(unit.id.clone(), unit);
        }
    }

    units
}

fn find_extractor() -> PathBuf {
    let test_exe = std::env::current_exe().expect("current_exe");
    let dir = test_exe.parent().unwrap().parent().unwrap();
    let candidate = dir.join("mdlr-extract-ts");
    if candidate.exists() {
        return candidate;
    }
    panic!(
        "Could not find mdlr-extract-ts binary at {}. \
         Run `cargo build --bin mdlr-extract-ts` first.",
        candidate.display()
    );
}

fn find_json_files(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(find_json_files(&path));
            } else if path.extension().is_some_and(|e| e == "json") {
                results.push(path);
            }
        }
    }
    results
}

// ---- Unit extraction tests ----

#[test]
fn function_declaration() {
    let units = extract(
        r#"
function greet(name: string): string {
    return "hello " + name;
}
"#,
    );

    let f = &units["src/test.ts::greet"];
    assert_eq!(f.kind, "Function");
    assert_eq!(f.params, 1);
}

#[test]
fn arrow_function_const() {
    let units = extract(
        r#"
const add = (a: number, b: number) => a + b;
"#,
    );

    let f = &units["src/test.ts::add"];
    assert_eq!(f.kind, "Function");
    assert_eq!(f.params, 2);
}

#[test]
fn function_expression_const() {
    let units = extract(
        r#"
const multiply = function(a: number, b: number) {
    return a * b;
};
"#,
    );

    let f = &units["src/test.ts::multiply"];
    assert_eq!(f.kind, "Function");
    assert_eq!(f.params, 2);
}

#[test]
fn class_and_methods() {
    let units = extract(
        r#"
class Foo {
    bar(x: number): number {
        return x * 2;
    }
}
"#,
    );

    let s = &units["src/test.ts::Foo"];
    assert_eq!(s.kind, "Struct");

    let m = &units["src/test.ts::Foo::bar"];
    assert_eq!(m.kind, "Method");
    assert_eq!(m.params, 1);
    assert_eq!(m.parent.as_deref(), Some("src/test.ts::Foo"));
}

#[test]
fn constructor() {
    let units = extract(
        r#"
class Widget {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
}
"#,
    );

    let c = &units["src/test.ts::Widget::constructor"];
    assert_eq!(c.kind, "Method");
    assert_eq!(c.params, 1);
    assert_eq!(c.parent.as_deref(), Some("src/test.ts::Widget"));
    assert!(c.writes.contains(&"name".to_string()));
}

#[test]
fn getter_setter() {
    let units = extract(
        r#"
class Box {
    private _value: number = 0;

    get value(): number {
        return this._value;
    }

    set value(v: number) {
        this._value = v;
    }
}
"#,
    );

    let getter = &units["src/test.ts::Box::get_value"];
    assert_eq!(getter.kind, "Method");
    assert_eq!(getter.params, 0);

    let setter = &units["src/test.ts::Box::set_value"];
    assert_eq!(setter.kind, "Method");
    assert_eq!(setter.params, 1);
}

#[test]
fn export_default_function() {
    let units = extract(
        r#"
export default function handler() {
    console.log("hello");
}
"#,
    );

    let f = &units["src/test.ts::handler"];
    assert_eq!(f.kind, "Function");
    assert!(f.calls.contains(&"console.log".to_string()));
}

#[test]
fn export_default_arrow() {
    let units = extract(
        r#"
export default () => {
    return 42;
};
"#,
    );

    let f = &units["src/test.ts::default"];
    assert_eq!(f.kind, "Function");
}

#[test]
fn nested_named_function() {
    let units = extract(
        r#"
function outer() {
    function inner() {
        return 42;
    }
    return inner();
}
"#,
    );

    assert!(units.contains_key("src/test.ts::outer"));
    assert!(units.contains_key("src/test.ts::outer::inner"));
}

// ---- Branch counting tests ----

#[test]
fn branches_if() {
    let units = extract(
        r#"
function branchy(x: number): number {
    if (x > 0) {
        return 1;
    } else if (x < 0) {
        return -1;
    } else {
        return 0;
    }
}
"#,
    );

    let f = &units["src/test.ts::branchy"];
    // if (+1) + else-if (+1) = 2
    assert_eq!(f.branches, 2, "if/else-if should count 2 branches");
}

#[test]
fn branches_switch() {
    let units = extract(
        r#"
function switchy(x: number): string {
    switch (x) {
        case 1: return "one";
        case 2: return "two";
        case 3: return "three";
        default: return "other";
    }
}
"#,
    );

    let f = &units["src/test.ts::switchy"];
    // 4 cases - 1 = 3
    assert_eq!(f.branches, 3, "switch with 4 cases should count 3 branches");
}

#[test]
fn branches_loops() {
    let units = extract(
        r#"
function loopy(n: number): number {
    let sum = 0;
    for (let i = 0; i < n; i++) {
        sum += i;
    }
    return sum;
}
"#,
    );

    let f = &units["src/test.ts::loopy"];
    assert_eq!(f.branches, 1, "for loop should count 1 branch");
}

#[test]
fn branches_logical_operators() {
    let units = extract(
        r#"
function logic(a: boolean, b: boolean, c: boolean): boolean {
    return a && b || c;
}
"#,
    );

    let f = &units["src/test.ts::logic"];
    // && (+1) + || (+1) = 2
    assert_eq!(f.branches, 2, "&& and || should each count as a branch");
}

#[test]
fn branches_ternary() {
    let units = extract(
        r#"
const decide = (x: number) => x > 0 ? "yes" : "no";
"#,
    );

    let f = &units["src/test.ts::decide"];
    assert_eq!(f.branches, 1, "ternary should count 1 branch");
}

#[test]
fn branches_zero_for_simple_fn() {
    let units = extract(
        r#"
function simple(x: number): number {
    return x + 1;
}
"#,
    );

    let f = &units["src/test.ts::simple"];
    assert_eq!(f.branches, 0, "simple function should have 0 branches");
}

// ---- Scope measurement tests ----

#[test]
fn scope_if_body() {
    let units = extract(
        r#"
function scoped(x: number): number {
    if (x > 0) {
        const a = 1;
        const b = 2;
        const c = 3;
        return a + b + c;
    } else {
        return 0;
    }
}
"#,
    );

    let f = &units["src/test.ts::scoped"];
    assert!(
        f.max_scope_lines >= 4,
        "max_scope should be at least 4, got {}",
        f.max_scope_lines
    );
}

// ---- Call extraction tests ----

#[test]
fn calls_function() {
    let units = extract(
        r#"
function helper(): number { return 42; }

function caller(): number {
    return helper();
}
"#,
    );

    let f = &units["src/test.ts::caller"];
    assert!(
        f.calls.contains(&"helper".to_string()),
        "should call helper, got {:?}",
        f.calls
    );
}

#[test]
fn calls_method() {
    let units = extract(
        r#"
function doStuff() {
    console.log("hello");
    const arr = [1, 2, 3];
    arr.push(4);
}
"#,
    );

    let f = &units["src/test.ts::doStuff"];
    assert!(
        f.calls.contains(&"console.log".to_string()),
        "should call console.log, got {:?}",
        f.calls
    );
    assert!(
        f.calls.contains(&"arr.push".to_string()),
        "should call arr.push, got {:?}",
        f.calls
    );
}

#[test]
fn calls_new_expr() {
    let units = extract(
        r#"
function create() {
    return new Map();
}
"#,
    );

    let f = &units["src/test.ts::create"];
    assert!(
        f.calls.contains(&"Map".to_string()),
        "should call Map (new), got {:?}",
        f.calls
    );
}

#[test]
fn calls_no_calls() {
    let units = extract(
        r#"
function pure(x: number): number {
    return x * 2;
}
"#,
    );

    let f = &units["src/test.ts::pure"];
    assert!(f.calls.is_empty(), "should have no calls, got {:?}", f.calls);
}

// ---- Field access tests ----

#[test]
fn field_read() {
    let units = extract(
        r#"
class Point {
    x: number;
    y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    sum(): number {
        return this.x + this.y;
    }
}
"#,
    );

    let f = &units["src/test.ts::Point::sum"];
    assert!(f.reads.contains(&"x".to_string()), "should read x");
    assert!(f.reads.contains(&"y".to_string()), "should read y");
    assert!(f.writes.is_empty(), "should have no writes");
}

#[test]
fn field_write() {
    let units = extract(
        r#"
class Counter {
    count: number = 0;

    increment() {
        this.count += 1;
    }
}
"#,
    );

    let f = &units["src/test.ts::Counter::increment"];
    assert!(
        f.writes.contains(&"count".to_string()),
        "should write count, got {:?}",
        f.writes
    );
}

#[test]
fn field_this_method_not_read() {
    let units = extract(
        r#"
class Logger {
    count: number = 0;

    log() {
        this.count += 1;
        this.flush();
    }

    flush() {}
}
"#,
    );

    let f = &units["src/test.ts::Logger::log"];
    assert!(f.writes.contains(&"count".to_string()));
    assert!(f.calls.contains(&"this.flush".to_string()));
    // flush should NOT appear as a field read
    assert!(
        !f.reads.contains(&"flush".to_string()),
        "this.method() should not be a field read, got {:?}",
        f.reads
    );
}

// ---- Parameter counting ----

#[test]
fn params_function() {
    let units = extract(
        r#"
function threeParams(a: number, b: number, c: number): number {
    return a + b + c;
}
"#,
    );

    let f = &units["src/test.ts::threeParams"];
    assert_eq!(f.params, 3);
}

#[test]
fn params_no_params() {
    let units = extract(
        r#"
function noParams(): void {}
"#,
    );

    let f = &units["src/test.ts::noParams"];
    assert_eq!(f.params, 0);
}

// ---- JS file support ----

#[test]
fn javascript_file() {
    let units = extract_file(
        "src/app.js",
        r#"
function hello(name) {
    console.log("hello " + name);
}

class App {
    run() {
        hello("world");
    }
}
"#,
    );

    assert!(units.contains_key("src/app.js::hello"));
    assert!(units.contains_key("src/app.js::App"));
    assert!(units.contains_key("src/app.js::App::run"));
}
