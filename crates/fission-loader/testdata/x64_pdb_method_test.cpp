// PDB fixture for a C++ member function: its CodeView type (LF_MFUNCTION)
// keeps `this` out of the argument list, while the function's own symbols
// list `this` as its first parameter.
//
//   clang-cl --target=x86_64-pc-windows-msvc /c /Od /Z7 /GS- /GR- /Brepro \
//       -fdebug-compilation-dir=. -gno-codeview-command-line \
//       x64_pdb_method_test.cpp
//   lld-link /DEBUG /Brepro /NODEFAULTLIB /ENTRY:entry /SUBSYSTEM:CONSOLE \
//       /PDBSOURCEPATH:C:\src /PDB:x64_pdb_method_test.pdb \
//       /PDBALTPATH:x64_pdb_method_test.pdb x64_pdb_method_test.obj \
//       /OUT:x64_pdb_method_test.exe
//
// The reproducibility flags keep the build directory out of both files.
struct Widget {
    int width;
    long height;
    __declspec(noinline) long scale(int factor, long bias);
};

long Widget::scale(int factor, long bias) {
    return width * factor + height + bias;
}

extern "C" int entry(void) {
    Widget w = {3, 4};
    return (int)w.scale(2, 5);
}
