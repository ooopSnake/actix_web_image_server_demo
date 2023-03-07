fn main() {
    prost_build::compile_protos(&["protos/image_command.proto"], &["protos/"]).unwrap();
}
