use regex::Regex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process;
use walkdir::WalkDir;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct FunctionInfo {
    path: PathBuf,
    module_path: String,
    definition: String,
    line_number: usize,
    calls: HashSet<String>,
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "-h" || args[1] == "--help") {
        print_help();
        process::exit(0);
    }

    if args.len() < 3 {
        eprintln!(
            "Usage: {} <project_root> <function_name> [preferred_module] [output_file]",
            args[0]
        );
        process::exit(1);
    }

    let project_root = Path::new(&args[1]);
    let target_function = &args[2];
    let preferred_module = if args.len() > 3 { Some(&args[3]) } else { None };
    let output_file = if args.len() > 4 {
        Some(PathBuf::from(&args[4]))
    } else if args.len() > 3 && !args[3].contains('/') && !args[3].contains('\\') {
        Some(PathBuf::from(&args[3]))
    } else {
        None
    };

    // Collect all Rust files in the project
    let rust_files = collect_rust_files(project_root)?;
    eprintln!("Found {} Rust files in project", rust_files.len());

    // Build function definitions map with fully qualified names
    let mut function_definitions: HashMap<String, FunctionInfo> = HashMap::new();
    let mut module_functions: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for path in &rust_files {
        let module_path = extract_module_path(path, project_root);
        let (functions, _) = process_file(path, &module_path)?;

        for (name, info) in functions {
            // Store with fully qualified name (module::function)
            let qualified_name = format!("{}::{}", module_path, name);
            function_definitions.insert(qualified_name.clone(), info);

            // Store simple name to module mapping
            module_functions
                .entry(name.clone())
                .or_insert_with(Vec::new)
                .push((qualified_name, module_path.clone()));
        }
    }

    // Build function call relationships
    let mut function_calls: HashMap<String, HashSet<String>> = HashMap::new();
    for (qualified_name, info) in &function_definitions {
        let mut resolved_calls = HashSet::new();

        for called_fn in &info.calls {
            // Try to resolve the called function to its qualified name
            if let Some(options) = module_functions.get(called_fn) {
                if options.len() == 1 {
                    // Only one function with this name
                    resolved_calls.insert(options[0].0.clone());
                } else {
                    // Multiple functions with this name - prefer same module
                    let caller_module = qualified_name.rsplit_once("::").map(|(m, _)| m);
                    let same_module = options.iter().find(|(_, m)| caller_module == Some(m));

                    if let Some((full_name, _)) = same_module {
                        resolved_calls.insert(full_name.clone());
                    } else {
                        // Default to first one
                        resolved_calls.insert(options[0].0.clone());
                    }
                }
            }
        }

        function_calls.insert(qualified_name.clone(), resolved_calls);
    }

    // Find our target function with module preference
    let selected_function =
        match find_function(target_function, preferred_module, &module_functions) {
            Some(func) => func,
            None => {
                // Try to find a partial match
                let mut matches = Vec::new();
                for (name, variants) in &module_functions {
                    if name.contains(target_function) {
                        for (qualified_name, module) in variants {
                            matches.push((qualified_name.clone(), module.clone()));
                        }
                    }
                }

                if !matches.is_empty() {
                    eprintln!(
                        "Function '{}' not found. Did you mean one of these?",
                        target_function
                    );
                    let mut deduped_matches = HashSet::new();
                    for (i, (qualified_name, module)) in matches.iter().enumerate() {
                        if i < 10 && deduped_matches.insert(qualified_name) {
                            eprintln!("  {} (in {})", qualified_name, module);
                        }
                    }
                    if matches.len() > 10 {
                        eprintln!("  ... and {} more", matches.len() - 10);
                    }
                } else {
                    eprintln!("Function '{}' not found in project", target_function);
                }

                process::exit(1);
            }
        };

    eprintln!("Selected function: {}", selected_function);

    // Find our target function and recursively gather all context
    let mut output = String::new();

    // Start with target function
    let mut queue = VecDeque::new();
    queue.push_back(selected_function.clone());
    let mut visited = HashSet::new();

    while let Some(current_function) = queue.pop_front() {
        if visited.contains(&current_function) {
            continue;
        }

        visited.insert(current_function.clone());

        if let Some(function_info) = function_definitions.get(&current_function) {
            let path_str = function_info.path.to_string_lossy();

            output.push_str(&format!("\n=== {} ===\n", path_str));
            output.push_str(&function_info.definition);
            output.push_str("\n\n");

            // Add all functions called by this function to the queue
            if let Some(called_fns) = function_calls.get(&current_function) {
                for called_fn in called_fns {
                    queue.push_back(called_fn.clone());
                }
            }
        }
    }

    // Either print to stdout or write to file
    if let Some(output_path) = output_file {
        let mut file = File::create(output_path)?;
        file.write_all(output.as_bytes())?;
        println!("Output written to file");
    } else {
        print!("{}", output);
    }

    Ok(())
}

fn find_function(
    target_function: &str,
    preferred_module: Option<&String>,
    module_functions: &HashMap<String, Vec<(String, String)>>,
) -> Option<String> {
    // Check if the function exists
    if let Some(variants) = module_functions.get(target_function) {
        if variants.len() == 1 {
            // Only one variant exists
            return Some(variants[0].0.clone());
        }

        // Multiple variants - try to match preferred module
        if let Some(module) = preferred_module {
            for (qualified_name, mod_path) in variants {
                if mod_path.contains(module) {
                    eprintln!("Found function in preferred module: {}", mod_path);
                    return Some(qualified_name.clone());
                }
            }

            // Print available modules
            eprintln!(
                "Function '{}' not found in module '{}'. Available in:",
                target_function, module
            );
            for (_, mod_path) in variants {
                eprintln!("  {}", mod_path);
            }

            // Default to first one
            eprintln!("Using first available implementation");
            return Some(variants[0].0.clone());
        }

        // No preferred module - list options
        eprintln!("Multiple implementations of '{}' found:", target_function);
        for (i, (_, module)) in variants.iter().enumerate() {
            eprintln!("  {}. In {}", i + 1, module);
        }
        eprintln!("Please specify a preferred module with the third argument");
        return Some(variants[0].0.clone());
    }

    None
}

fn collect_rust_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut rust_files = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
    {
        let path = entry.path();
        if let Some(extension) = path.extension() {
            if extension == "rs" {
                rust_files.push(path.to_path_buf());
            }
        }
    }

    Ok(rust_files)
}

fn extract_module_path(file_path: &Path, project_root: &Path) -> String {
    let rel_path = file_path.strip_prefix(project_root).unwrap_or(file_path);
    let path_str = rel_path.to_string_lossy();

    // Convert path to Rust module path format
    let mut module_path = path_str
        .replace('/', "::")
        .replace('\\', "::")
        .replace(".rs", "");

    // Special case for lib.rs or mod.rs
    if module_path.ends_with("::lib") {
        module_path = module_path[..module_path.len() - 5].to_string();
    } else if module_path.ends_with("::mod") {
        module_path = module_path[..module_path.len() - 5].to_string();
    }

    module_path
}

fn process_file(
    path: &Path,
    module_path: &str,
) -> io::Result<(HashMap<String, FunctionInfo>, HashSet<String>)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let content: String = reader
        .lines()
        .filter_map(Result::ok)
        .collect::<Vec<String>>()
        .join("\n");

    let mut function_info: HashMap<String, FunctionInfo> = HashMap::new();
    let mut types: HashSet<String> = HashSet::new();

    // Extract function definitions with their body
    let fn_regex =
        Regex::new(r"(?m)^\s*(pub\s+)?(async\s+)?fn\s+([a-zA-Z0-9_]+)\s*(<.*?>)?\s*\(").unwrap();

    for captures in fn_regex.captures_iter(&content) {
        let function_name = captures.get(3).unwrap().as_str();
        let line_number = content[..captures.get(0).unwrap().start()].lines().count() + 1;

        let mut def_start = captures.get(0).unwrap().start();
        while def_start > 0 && !content[def_start - 1..def_start].contains('\n') {
            def_start -= 1;
        }

        // Find the function's closing brace by properly tracking nested braces
        let mut brace_count = 0;
        let mut found_opening_brace = false;
        let mut def_end = captures.get(0).unwrap().end();

        for (i, c) in content[def_end..].chars().enumerate() {
            if c == '{' {
                found_opening_brace = true;
                brace_count += 1;
            } else if c == '}' {
                brace_count -= 1;
                if brace_count == 0 && found_opening_brace {
                    def_end += i + 1;
                    break;
                }
            }
        }

        // If we couldn't find the end properly, just use a large chunk
        if !found_opening_brace || brace_count != 0 {
            def_end = std::cmp::min(def_end + 5000, content.len());
        }

        let fn_body = content[def_start..def_end].trim().to_string();

        // Extract function calls within this function body
        let mut calls = HashSet::new();

        // Look for method calls (.method())
        let method_regex = Regex::new(r"\.([a-zA-Z0-9_]+)\s*\(").unwrap();
        for method_captures in method_regex.captures_iter(&fn_body) {
            let method_name = method_captures.get(1).unwrap().as_str();
            // Skip common built-ins and add the rest
            if ![
                "is_empty",
                "len",
                "clone",
                "unwrap",
                "unwrap_or",
                "unwrap_or_else",
                "expect",
                "map",
                "map_err",
                "and_then",
                "or_else",
                "filter",
                "collect",
                "to_string",
                "to_str",
                "parse",
                "as_str",
                "as_ref",
                "display",
                "send",
                "await",
                "lock",
                "get",
                "push",
                "pop",
                "clear",
                "insert",
                "contains_key",
            ]
            .contains(&method_name)
            {
                calls.insert(method_name.to_string());
            }
        }

        // Look for function calls (function())
        let call_regex = Regex::new(r"[^a-zA-Z0-9_\.]([a-zA-Z0-9_]+)\s*\(").unwrap();
        for call_captures in call_regex.captures_iter(&fn_body) {
            let called_function = call_captures.get(1).unwrap().as_str();

            // Skip known keywords, macros, and builtins
            if [
                "if", "for", "while", "match", "return", "assert", "println", "panic", "format",
                "print", "info", "error", "warn", "debug", "trace", "let", "break", "continue",
                "loop", "async", "await", "move", "static", "const", "struct", "enum", "trait",
                "impl", "type", "pub", "self", "map", "filter", "as", "is", "mut", "ref", "vec",
                "super", "use", "extern", "spawn", "process", "eprintln", "unwrap",
            ]
            .contains(&called_function)
            {
                continue;
            }

            calls.insert(called_function.to_string());
        }

        // Look for AWS SDK builder pattern calls
        let builder_regex = Regex::new(r"([a-zA-Z0-9_]+)\s*\(\s*\)").unwrap();
        for builder_captures in builder_regex.captures_iter(&fn_body) {
            let builder_fn = builder_captures.get(1).unwrap().as_str();
            if ![
                "Ok", "Err", "Some", "None", "Arc", "Vec", "HashMap", "HashSet", "String",
            ]
            .contains(&builder_fn)
            {
                calls.insert(builder_fn.to_string());
            }
        }

        function_info.insert(
            function_name.to_string(),
            FunctionInfo {
                path: path.to_path_buf(),
                module_path: module_path.to_string(),
                definition: fn_body,
                line_number,
                calls,
            },
        );
    }

    // Also extract struct/enum/type definitions
    let type_regex =
        Regex::new(r"(?m)^\s*(pub\s+)?(struct|enum|type|trait)\s+([a-zA-Z0-9_]+)").unwrap();

    for captures in type_regex.captures_iter(&content) {
        let type_name = captures.get(3).unwrap().as_str();
        types.insert(type_name.to_string());
    }

    Ok((function_info, types))
}

fn print_help() {
    println!("Function Context Analyzer - Extract function call trees from Rust projects");
    println!("\nUSAGE:");
    println!("  context-analyzer <project_root> <function_name> [preferred_module] [output_file]");
    println!("\nARGUMENTS:");
    println!("  <project_root>     Path to the Rust project root directory");
    println!("  <function_name>    Name of the function to analyze");
    println!("  [preferred_module] Optional module name to disambiguate functions");
    println!("  [output_file]      Optional output file path (defaults to stdout)");
    println!("\nEXAMPLES:");
    println!("  context-analyzer ./my-project process_queue transform_writer output.txt");
    println!("  context-analyzer ./my-project main");
}
