extern crate cc;

fn main() {
  cc::Build::new()
    .cpp(true)
    .include("ccompile/targetver.h")
    .include("ccompile/stdafx.h")
    .file("ccompile/stdafx.cpp")
    .include("ccompile/native.h")
    .file("ccompile/native.cpp")
    .compile("native");
}

