/** Compiled by the Cargo build system to test if the libstapsdt headers and lib are
 * in the path or not. */

#include <libstapsdt.h>

int main(int arc, char** argv) {
    SDTProvider_t* provider = providerInit("foo");
    providerDestroy(provider);
}
