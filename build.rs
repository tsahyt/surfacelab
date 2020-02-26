extern crate shaderc;
use std::{fs, io::Write};

fn main() {
    // initialize compiler
    let mut compiler = shaderc::Compiler::new().unwrap();

    // process all shaders
    for entry in fs::read_dir("shaders").unwrap() {
        let entry = entry.unwrap();

        // skip all already compiled files
        if entry.path().extension().unwrap().to_str() == Some("spv") {
            continue;
        }

        // load shader and compile
        let shader = fs::read_to_string(entry.path()).unwrap();
        let binary_result = compiler.compile_into_spirv(
            &shader,
            match entry.path().extension().unwrap().to_str().unwrap() {
                "comp" => shaderc::ShaderKind::Compute,
                "frag" => shaderc::ShaderKind::Fragment,
                "vert" => shaderc::ShaderKind::Vertex,
                _ => {
                    eprintln!("Illegal extension discovered");
                    std::process::exit(1);
                }
            },
            entry.path().file_name().unwrap().to_str().unwrap(),
            "main",
            None,
        );

        // save to .spv file on success
        match binary_result {
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            Ok(binary_result) => {
                let mut output_path = entry.path().clone();
                output_path.set_extension("spv");

                let mut output = fs::File::create(output_path).unwrap();
                output.write_all(binary_result.as_binary_u8()).unwrap();
            }
        }
    }
}
