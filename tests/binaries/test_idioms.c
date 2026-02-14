#include <stdio.h>

long long mod_ll(long long a) {
    return a % 2;
}

long long div_ll(long long a) {
    return a / 3;
}

int main() {
    long long val = 123456789012345LL;
    printf("mod: %lld, div: %lld\n", mod_ll(val), div_ll(val));
    return 0;
}
