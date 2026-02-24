/// Name conversion utilities for IDL code generation.

/// Convert to PascalCase for Rust type names.
/// "hello_world" -> "HelloWorld"
/// "SENSOR_KIND" -> "SensorKind"
/// "SensorData" -> "SensorData" (already PascalCase)
pub fn to_pascal_case(name: &str) -> String {
    if name.contains('_') {
        name.split('_')
            .filter(|s| !s.is_empty())
            .map(|part| {
                let lower = part.to_lowercase();
                let mut chars = lower.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    } else if name.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()) {
        // ALL_CAPS -> Allcaps
        let lower = name.to_lowercase();
        let mut chars = lower.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
    } else {
        // Already PascalCase or camelCase - ensure first char is uppercase
        let mut chars = name.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
    }
}

/// Convert to snake_case for Rust field/module names.
/// "HelloWorld" -> "hello_world"
/// "SensorData" -> "sensor_data"
/// "sensor_id" -> "sensor_id" (already snake_case)
pub fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    let mut prev_upper = false;
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !prev_upper {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
            prev_upper = true;
        } else {
            prev_upper = false;
            result.push(c);
        }
    }
    result
}

/// Convert to SCREAMING_SNAKE_CASE for C enum prefixes.
/// "SensorKind" -> "SENSOR_KIND"
pub fn to_screaming_snake(name: &str) -> String {
    to_snake_case(name).to_uppercase()
}

/// Convert IDL filename to output base name.
/// "HelloWorld.idl" -> "hello_world"
pub fn idl_to_output_name(filename: &str) -> String {
    let stem = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .strip_suffix(".idl")
        .unwrap_or(filename);
    to_snake_case(stem)
}

/// Generate C include guard from filename.
/// "hello_world.h" -> "HELLO_WORLD_H"
pub fn to_include_guard(filename: &str) -> String {
    let stem = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename);
    stem.replace('.', "_").to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("SENSOR_KIND"), "SensorKind");
        assert_eq!(to_pascal_case("SensorData"), "SensorData");
        assert_eq!(to_pascal_case("RED"), "Red");
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("SensorData"), "sensor_data");
        assert_eq!(to_snake_case("sensor_id"), "sensor_id");
    }

    #[test]
    fn test_screaming_snake() {
        assert_eq!(to_screaming_snake("SensorKind"), "SENSOR_KIND");
        assert_eq!(to_screaming_snake("Color"), "COLOR");
    }

    #[test]
    fn test_output_name() {
        assert_eq!(idl_to_output_name("HelloWorld.idl"), "hello_world");
        assert_eq!(idl_to_output_name("path/to/SensorData.idl"), "sensor_data");
    }

    #[test]
    fn test_include_guard() {
        assert_eq!(to_include_guard("hello_world.h"), "HELLO_WORLD_H");
    }
}
