#!/bin/bash
VAGRANT_URL=https://releases.hashicorp.com/vagrant/2.2.5/vagrant_2.2.5_x86_64.rpm
TMP_DIR=/tmp/vagrant-init

set -e
######### Install VirtualBox ##########
# https://www.tecmint.com/install-virtualbox-in-fedora-linux/
wget http://download.virtualbox.org/virtualbox/rpm/fedora/virtualbox.repo -P /etc/yum.repos.d/
dnf update
dnf install -y @development-tools
dnf install -y kernel-devel kernel-headers dkms qt5-qtx11extras  elfutils-libelf-devel zlib-devel
dnf install -y VirtualBox-6.0

######### Install Vagrant ##########
mkdir $TMP_DIR
cd $TMP_DIR;

wget $VAGRANT_URL

rpm -i vagrant*

# cleanup
rm $TMP_DIR/vagrant*
rmdir $TMP_DIR