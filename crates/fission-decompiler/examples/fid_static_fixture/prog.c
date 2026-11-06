#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <math.h>
#include <time.h>
#include <regex.h>
#include <getopt.h>
#include <locale.h>
#include <signal.h>
#include <dirent.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <wchar.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <pwd.h>
#include <netdb.h>
#include <iconv.h>
#include <glob.h>
#include <fnmatch.h>
#include <search.h>
static int cmp(const void *a, const void *b) { return *(const int *)a - *(const int *)b; }
int main(int argc, char **argv) {
    int opt; while ((opt = getopt(argc, argv, "ab:")) != -1) printf("opt %c\n", opt);
    setlocale(LC_ALL, "");
    int v[16]; for (int i = 0; i < 16; i++) v[i] = rand() % 100;
    qsort(v, 16, sizeof v[0], cmp);
    int key = v[3]; int *f = bsearch(&key, v, 16, sizeof v[0], cmp);
    printf("%d %p %f %f %f\n", v[0], (void *)f, sin(argc), exp(argc), pow(argc, 2.5));
    char buf[256]; snprintf(buf, sizeof buf, "%s-%d-%5.2f", argv[0], argc, log(argc + 1.0));
    time_t t = time(NULL); struct tm *tm = localtime(&t); strftime(buf, sizeof buf, "%Y-%m-%d %H:%M:%S", tm); puts(buf);
    regex_t re; if (!regcomp(&re, "a+b*", REG_EXTENDED)) { printf("%d\n", regexec(&re, buf, 0, NULL, 0)); regfree(&re); }
    FILE *fp = fopen("/etc/passwd", "r"); if (fp) { while (fgets(buf, sizeof buf, fp)) { long l = strtol(buf, NULL, 10); double d = strtod(buf, NULL); printf("%ld %f %s\n", l, d, strtok(buf, ":")); } fclose(fp); }
    DIR *d = opendir("."); struct dirent *e; if (d) { while ((e = readdir(d))) puts(e->d_name); closedir(d); }
    struct stat st; if (!stat("/", &st)) printf("%ld\n", (long)st.st_size);
    struct passwd *pw = getpwnam("root"); if (pw) puts(pw->pw_dir);
    glob_t g; if (!glob("*.c", 0, NULL, &g)) globfree(&g);
    printf("%d\n", fnmatch("*.c", "a.c", 0));
    wchar_t w[32]; mbstowcs(w, "hello", 32); printf("%zu\n", wcslen(w));
    char *p = malloc(64); p = realloc(p, 4096); strcpy(p, "x"); strcat(p, "yz"); printf("%s %zu %s\n", p, strlen(p), strchr(p, 'y')); free(p);
    signal(SIGINT, SIG_IGN); struct timeval tv; gettimeofday(&tv, NULL);
    ENTRY item = { "k", "v" }; hcreate(10); hsearch(item, ENTER);
    iconv_t cd = iconv_open("UTF-8", "ISO-8859-1"); if (cd != (iconv_t)-1) iconv_close(cd);
    struct hostent *h = gethostbyname("localhost"); if (h) puts(h->h_name);
    for (char *c = buf; *c; c++) *c = toupper(*c);
    return (int)(tv.tv_sec & 1) + errno;
}
