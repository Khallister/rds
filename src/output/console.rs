use crate::types::DependencyTree;
use console::style;
use std::collections::HashMap;

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
            let normalized_entry = self.normalize_path_for_display(entry);

            let matching_key = tree
                .keys()
                .find(|key| {
                    let normalized_key = self.normalize_path_for_display(key);
                    normalized_key == normalized_entry
                        || key.ends_with(&normalized_entry)
                        || normalized_entry.ends_with(&normalized_key)
                })
                .cloned()
                .unwrap_or_else(|| entry.clone());

            self.print_node(
                &matching_key,
                "  ",
                tree,
                &mut id_map,
                &mut id_counter,
                digits,
                false,
            );
        }

        println!();
    }

    fn normalize_path_for_display(&self, path: &str) -> String {
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
        let line = format!(
            "{}{}",
            style(format!("{}- {}) ", prefix, id_str)).dim(),
            node_id
        );

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
                self.print_node(
                    dep_id,
                    &new_prefix,
                    tree,
                    id_map,
                    id_counter,
                    digits,
                    !is_last,
                );
            }
        } else {
            println!("{}", style(line).yellow());
        }
    }

    ///
    pub fn print_circular(
        &self,
        circulars: &[Vec<String>],
        take_limit: Option<usize>,
        max_entries: Option<usize>,
    ) {
        let header = if circulars.is_empty() {
            style("🔄 Circular Dependencies").bold().green()
        } else {
            style("⚠️  Circular Dependencies").bold().red()
        };

        println!("{}", header);

        if circulars.is_empty() {
            println!(
                "  {} {}",
                style("✅").green(),
                style("Congratulations, no circular dependency was found in your project.").green()
            );
        } else {
            let digits = circulars.len().to_string().len();
            let to_show = max_entries.unwrap_or(circulars.len());

            for (i, circular) in circulars.iter().enumerate().take(to_show) {
                print!("  {:0width$}) ", i + 1, width = digits);

                for (j, seg) in circular.iter().enumerate() {
                    print!("{}", style(seg).red().bold());
                    if j != circular.len() - 1 {
                        print!("{}", style(" → ").dim());
                    }
                }

                println!();
            }

            if circulars.len() > to_show {
                println!("  ... and {} more", circulars.len() - to_show);
            }

            if let Some(limit) = take_limit {
                if circulars.len() >= limit {
                    println!(
                        "  {} {} (search limit reached)",
                        style("At least").dim(),
                        style(format!("{} circular dependencies found", limit)).bold()
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
                println!(
                    "  {:0width$}) {}",
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
            println!("  {}", style("✅ No unused files found").bold().green());
        } else {
            let digits = unused.len().to_string().len();
            for (i, file) in unused.iter().enumerate() {
                println!("  {:0width$}) {}", i + 1, file, width = digits);
            }
        }

        println!();
    }

    fn is_builtin_module(&self, module: &str) -> bool {
        matches!(
            module,
            "assert"
                | "buffer"
                | "child_process"
                | "cluster"
                | "crypto"
                | "dgram"
                | "dns"
                | "domain"
                | "events"
                | "fs"
                | "http"
                | "https"
                | "module"
                | "net"
                | "os"
                | "path"
                | "punycode"
                | "querystring"
                | "readline"
                | "stream"
                | "string_decoder"
                | "timers"
                | "tls"
                | "tty"
                | "url"
                | "util"
                | "vm"
                | "zlib"
        )
    }
}
