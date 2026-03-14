#!/bin/bash

echo -e "\e[34mmochiOS dependencies auto installer\e[0m"
echo "This script is for Ubuntu. It automatically uses apt and also installs homebrew."
echo "Please install homebrew."
echo "The components installed by this script are as described in the README."
echo "Cargo-related tools are not installed automatically."
read -p "Continue? [y/n]: " answer

case "$answer" in
    [yY] | [yY][eE][sS] )
        echo "Starting installation..."
        ;;
    * )
        echo "ok goodluck! :)"
        exit 1
        ;;
esac

if ! command -v brew &> /dev/null; then
    echo -e "\e[31mError: Please install homebrew: https://brew.sh\e[0m"
    exit 1
fi

sudo apt update
sudo apt upgrade -y
sudo apt install -y git
sudo apt install -y qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils virt-manager
sudo apt install -y mtools
sudo apt install -y build-essential
sudo apt install -y make
sudo apt install -y libgcc-s1
brew install x86_64-elf-gcc

echo ""
echo -e "\e[32mInstallation completed successfully! :D\e[0m"