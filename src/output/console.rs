use console::{style};
use std::collections::HashMap;
use crate::types::DependencyTree;

pub struct ConsoleOutput;

impl ConsoleOutput {
    pub fn new() -> Self {
        Self
    }
    
    pub fn print_tree(&self, tree: &DependencyTree, entries: &[String]) {
        println!("{}", style("🌳 Dependencies Tree").bold().cyan());
        
        let mut id_map = HashMap::new();
        let mut id_counter = 0;
        let digits = tree.len().to_string().len();
        
        for entry in entries {
            // Normalize the entry path to match tree keys
            let normalized_entry = self.normalize_path_for_display(entry);
            
            // Find the matching key in the tree
            let matching_key = tree.keys()
                .find(|key| {
                    let normalized_key = self.normalize_path_for_display(key);
                    normalized_key == normalized_entry || 
                    key.ends_with(&normalized_entry) || 
                    normalized_entry.ends_with(&normalized_key)
                })
                .cloned()
                .unwrap_or_else(|| entry.clone());
                
            self.print_node(&matching_key, "  ", tree, &mut id_map, &mut id_counter, digits, false);
        }
        
        println!();
    }
    
    fn normalize_path_for_display(&self, path: &str) -> String {
        // Convert path separators and remove redundant ./ patterns
        path.replace('/', "\\")
            .replace("\\.\\", "\\")
            .trim_start_matches(".\\")
            .trim_start_matches("./")
            .to_string()
    }
    
    fn print_node(
        &self,
        node_id: &str,
        prefix: &str,
        tree: &DependencyTree,
        id_map: &mut HashMap<String, usize>,
        id_counter: &mut usize,
        digits: usize,
        has_more: bool,
    ) {
        let is_new = !id_map.contains_key(node_id);
        let id = *id_map.entry(node_id.to_string()).or_insert_with(|| {
            let current = *id_counter;
            *id_counter += 1;
            current
        });
        
        let id_str = format!("{:0width$}", id, width = digits);
        let line = format!("{}{}",
            style(format!("{}- {}) ", prefix, id_str)).dim(),
            node_id
        );
        
        // Check if it's a built-in module
        if self.is_builtin_module(node_id) {
            println!("{}", style(line).blue());
            return;
        }
        
        if !is_new {
            println!("{}", style(line).dim());
            return;
        }
        
        if let Some(Some(deps)) = tree.get(node_id) {
            println!("{}", line);
            let new_prefix = format!("{}{}   ", prefix, if has_more { "·" } else { " " });
            
            for (i, dep) in deps.iter().enumerate() {
                let dep_id = dep.id.as_ref().unwrap_or(&dep.request);
                let is_last = i == deps.len() - 1;
                self.print_node(dep_id, &new_prefix, tree, id_map, id_counter, digits, !is_last);
            }
        } else {
            println!("{}", style(line).yellow());
        }
    }
    
    pub fn print_circular(&self, circulars: &[Vec<String>], take_limit: Option<usize>) {
        let header = if circulars.is_empty() {
            style("🔄 Circular Dependencies").bold().green()
        } else {
            style("⚠️  Circular Dependencies").bold().red()
        };
        
        println!("{}", header);
        
        if circulars.is_empty() {
            println!("  {}", 
                style("✅ Congratulations, no circular dependency was found in your project.")
                    .bold().green()
            );
        } else {
            let digits = circulars.len().to_string().len();
            for (i, circular) in circulars.iter().enumerate() {
                let line = format!("  {:0width$}) {}", 
                    i + 1, 
                    circular.iter()
                        .map(|s| style(s).red().to_string())
                        .collect::<Vec<_>>()
                        .join(&style(" -> ").dim().to_string()),
                    width = digits
                );
                println!("{}", style(line).dim());
            }
            
            // Show "at least N" message if we hit the take limit
            if let Some(limit) = take_limit {
                if circulars.len() >= limit {
                    println!("  {}", 
                        style(format!("At least {} circular dependencies found (search limit reached)", limit))
                            .bold().yellow()
                    );
                }
            }
        }
        
        println!();
    }
    
    pub fn print_warnings(&self, warnings: &[String]) {
        println!("{}", style("• Warnings").bold().yellow());
        
        if warnings.is_empty() {
            println!("  No warnings");
        } else {
            let digits = warnings.len().to_string().len();
            for (i, warning) in warnings.iter().enumerate() {
                println!("  {:0width$}) {}", 
                    i + 1, 
                    style(warning).yellow(),
                    width = digits
                );
            }
        }
        
        println!();
    }
    
    pub fn print_unused_files(&self, unused: &[String]) {
        println!("{}", style("• Unused files").bold().cyan());
        
        if unused.is_empty() {
            println!("  {}", 
                style("✅ No unused files found").bold().green()
            );
        } else {
            let digits = unused.len().to_string().len();
            for (i, file) in unused.iter().enumerate() {
                println!("  {:0width$}) {}", i + 1, file, width = digits);
            }
        }
        
        println!();
    }
    
    fn is_builtin_module(&self, module: &str) -> bool {
        matches!(module, 
            "assert" | "buffer" | "child_process" | "cluster" | "crypto" | "dgram" |
            "dns" | "domain" | "events" | "fs" | "http" | "https" | "module" | "net" |
            "os" | "path" | "punycode" | "querystring" | "readline" | "stream" |
            "string_decoder" | "timers" | "tls" | "tty" | "url" | "util" | "vm" | "zlib"
        )
    }
}
