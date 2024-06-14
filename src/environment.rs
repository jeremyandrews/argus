use std::env;

/// Retrieves an environment variable and splits it into a vector of strings based on a delimiter.
///
/// # Arguments
/// - `var`: The name of the environment variable.
/// - `delimiter`: The character to split the environment variable's value by.
///
/// # Returns
/// - `Vec<String>`
pub fn get_env_var_as_vec(var: &str, delimiter: char) -> Vec<String> {
    env::var(var)
        .unwrap_or_default()
        .split(delimiter)
        .map(|s| s.trim().to_string())
        .collect()
}
