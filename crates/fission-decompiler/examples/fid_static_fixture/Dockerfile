FROM --platform=linux/amd64 centos:7
# CentOS 7 moved to the vault; 7.7.1908 carries glibc-static 2.17-292.el7, the build the libc pack holds.
RUN sed -i 's|^mirrorlist=|#mirrorlist=|; s|^#baseurl=http://mirror.centos.org/centos/\$releasever|baseurl=https://vault.centos.org/7.7.1908|' /etc/yum.repos.d/CentOS-Base.repo \
 && yum -y downgrade glibc-2.17-292.el7 glibc-common-2.17-292.el7 >/dev/null \
 && yum -y install gcc glibc-static-2.17-292.el7 glibc-devel-2.17-292.el7 glibc-headers-2.17-292.el7 binutils make >/dev/null \
 && rpm -q glibc glibc-static gcc
