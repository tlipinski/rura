use std::process::Command;

pub enum CompletionType {
    Command,
    File,
}

pub fn get_completions(input: &str, kind: CompletionType) -> Vec<String> {
    let flag = match kind {
        CompletionType::Command => "-c",
        CompletionType::File => "-f",
    };

    // Use bash -c "compgen ..." to get completions.
    // We need to escape the input to prevent shell injection, although this is a local tool.
    // For simplicity, we just pass it as an argument to compgen.
    let output = Command::new("bash")
        .arg("-c")
        .arg(format!("compgen {} \"$1\"", flag))
        .arg("--")
        .arg(input)
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![]
    }
}
