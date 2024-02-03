use anyhow::Context;
use anyhow::Result;
use std::{error::Error, path::Path};

use clap::Parser;
use ignore::WalkBuilder;
use tree_sitter::{Node, Tree};

#[derive(Debug, Parser)]
struct Cli {
    // take in pattern as last argument with no default
    pattern: String,
}

fn main() -> Result<()> {
    // Parse the command line arguments
    let args = Cli::parse();
    let pattern = args.pattern;

    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_python::language();
    parser
        .set_language(language)
        .expect("Error loading Python grammar");

    // Walk the current directory
    for result in WalkBuilder::new("./").build() {
        let entry = result?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("py") {
            // Read the file content
            let source_code = std::fs::read_to_string(path).expect("Error reading file");

            // Process the file with tree-sitter
            let tree = parser
                .parse(&source_code, None)
                .context("Error parsing source code")?;
            let root_node = tree.root_node();
            process_file(&root_node, &source_code, path, &pattern)?;
        }
    }

    Ok(())
}

fn process_file(root_node: &Node, source_code: &str, fname: &Path, pattern: &str) -> Result<()> {
    let matched_nodes = find_leaf_nodes_with_text(*root_node, pattern, source_code)?;
    if matched_nodes.is_empty() {
        return Ok(());
    }
    for matched_node in matched_nodes {
        let hierarchy = collect_parent_hierarchy(matched_node);

        let nodes_with_names: Vec<(Node, String)> = hierarchy
            .iter()
            .filter_map(|n| get_node_name(*n, source_code).map(|name| (*n, name)))
            .collect();

        let last_node = nodes_with_names.last().map(|(n, _)| *n).unwrap();

        let hierarchy_str = hierarchy
            .iter()
            .filter_map(|n| get_node_name(*n, source_code))
            .collect::<Vec<_>>()
            .join("->");

        let line = matched_block(matched_node, last_node, source_code);
        println!("{} {}: {}", fname.to_string_lossy(), hierarchy_str, line);
    }
    Ok(())
}

fn find_leaf_nodes_with_text<'a>(
    node: Node<'a>,
    search_term: &str,
    source_code: &str,
) -> Result<Vec<Node<'a>>> {
    let mut matches = Vec::new();
    if node.child_count() == 0 {
        let node_text = node.utf8_text(source_code.as_bytes())?;
        if node_text.contains(search_term) {
            matches.push(node);
        }
    } else {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                matches.extend(find_leaf_nodes_with_text(child, search_term, source_code)?);
            }
        }
    }
    Ok(matches)
}

fn collect_parent_hierarchy(node: Node) -> Vec<Node> {
    let mut current_node = Some(node);
    let mut hierarchy = Vec::new();

    while let Some(node) = current_node {
        hierarchy.push(node);
        current_node = node.parent();
    }

    hierarchy.reverse(); // Reverse to get the hierarchy from root to leaf
    hierarchy
}
fn get_node_name(node: Node, source_code: &str) -> Option<String> {
    match node.kind() {
        "class_definition" | "function_definition" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_code.as_bytes()).ok())
            .map(String::from),
        _ => None,
    }
}

fn matched_block(node: Node, last_node: Node, source_code: &str) -> String {
    // TODO we want to find the highest level node that we printed out previously and then report

    // Find the topmost relevant node for the pattern (e.g., the enclosing function or class)
    let mut relevant_node = node;
    loop {
        let Some(parent) = relevant_node.parent() else {
            break;
        };

        let Some(grandparent) = parent.parent() else {
            break;
        };

        if grandparent == last_node {
            break;
        }
        relevant_node = parent;
    }

    // Extract the entire block
    let start_byte = relevant_node.start_byte();
    let end_byte = relevant_node.end_byte();
    let block = &source_code[start_byte..end_byte];

    // remove newlines
    format_multiline(block)
}


fn format_multiline(s: &str) -> String {
    let re = regex::Regex::new(r"\n\s+").unwrap();

    re.replace_all(s, "↩").to_string()
}