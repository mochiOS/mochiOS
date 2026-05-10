#!/bin/bash
set -e

echo -e "\e[34mmochiOS dependencies auto installer\e[0m"
echo "This script targets Ubuntu and installs dependencies via apt."
echo "Homebrew is required for x86_64-elf-gcc and is checked (not auto-installed)."
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
sudo apt install -y qemu-system-x86_64 qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils virt-manager
sudo apt install -y mtools
sudo apt install -y e2fsprogs
sudo apt install -y build-essential
sudo apt install -y make
sudo apt install -y libgcc-s1
sudo apt install -y texinfo
brew install x86_64-elf-gcc

echo ""
echo -e "\e[32mInstallation completed successfully! :D\e[0m"