__attribute__((noinline, used)) int fission_variadic_fixture(
    int fixed, const char *format, ...) {
    return fixed + format[0];
}

void _start(void) {
    (void)fission_variadic_fixture(1, "x", 2, 3);
}
