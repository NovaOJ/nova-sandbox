#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define MB (128 * 1024 * 1024)

int main(int argc, char *argv[])
{
    char *p;
    int i = 0;
	for( int i = 1; i <= 100; i ++ ) {
        p = (char *)malloc(MB);
        memset(p, 0, MB);
    }

    return 0;
}
