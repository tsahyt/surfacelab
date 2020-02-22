extern crate shaderc;
use std::{fs, io::Write};

fn main() {
    // initialize compiler
    let mut compiler = shaderc::Compiler::new().unwrap();
    let mut options = shaderc::CompileOptions::new().unwrap();

    // process all compute shaders
    for entry in fs::read_dir("shaders").unwrap() {
        let entry = entry.unwrap();
        let shader = fs::read_to_string(entry.path()).unwrap();
        let binary_result = compiler.compile_into_spirv(
            &shader,
            shaderc::ShaderKind::Compute,
            entry.path().file_name().unwrap().to_str().unwrap(),
            "main",
            None,
        );

        match binary_result {
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            Ok(binary_result) => {
                let mut output_path = entry.path().clone();
                output_path.set_extension("spv");

                let mut output = fs::File::create(output_path).unwrap();
                output.write_all(binary_result.as_binary_u8());
            }
        }
    }
}
