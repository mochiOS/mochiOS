<div align="center">

# mochiOS

##### This is an OS. Do not eat.

The computer, made personal again.

[Website](https://www.mochios.org) · [Developer](https://developer.mochios.org) · [Contribute](./contributing.md)

</div>

## Built differently.

mochiOS is an operating system built from the ground up with isolation, least privilege, and recoverability at its core.

Applications, services, and drivers are separated from one another and given only the capabilities they need.

A failure should stay where it happened.

## Made to feel like one system.

The kernel, system services, application framework, interface, and development tools are designed together.

Not as separate pieces, but as one computer experience.

Simple where it should be simple.
Powerful where it needs to be.

## Development? Welcome!

mochiOS is developed in the open.

Its kernel, system components, frameworks, applications, and development tools are available across the [mochiOS GitHub organization](https://github.com/mochiOS).

Learn more at [developer.mochios.org](https://developer.mochios.org).

## Build

mochiOS uses `repo` to manage its source tree.

The supported build host is Ubuntu 24.04. Install the host tools used by the
build and QEMU runner:

```sh
sudo apt-get update
sudo apt-get install --yes \
    autoconf automake binutils bison build-essential curl dosfstools \
    e2fsprogs fakeroot flex gawk libgmp-dev libmpc-dev libmpfr-dev \
    libtool mtools openssl ovmf perl qemu-system-x86 repo texinfo \
    xz-utils
```

The kernel and boot components require the pinned Rust nightly toolchain and
its Rust source component:

```sh
rustup toolchain install nightly-2026-05-14 \
    --profile minimal \
    --component rust-src
```

The userland C runtime requires an `x86_64-elf` cross compiler. On Linuxbrew,
install it with:

```sh
brew install x86_64-elf-gcc
```

Verify that `x86_64-elf-gcc`, `x86_64-elf-ar`, `x86_64-elf-ranlib`,
`x86_64-elf-ld`, `x86_64-elf-nm`, and `x86_64-elf-readelf` are on `PATH`.

```sh
git clone https://github.com/mochiOS/mochiOS.git
cd mochiOS

make repo-init
make build
```

The bootable raw disk image is written to `out/artifacts/disk.img`.

## Run with QEMU

mochiOS can be started directly for development, or through mBoot to test the complete system architecture.

### Run mochiOS directly

`make run` builds the current tree and starts mochiOS directly with
`qemu-system-x86_64`.

The default configuration uses KVM and opens a QEMU display window:

```sh
make run
```

The user running QEMU must have read/write access to `/dev/kvm`.

To run on a host without KVM, use QEMU's TCG accelerator:

```sh
QEMU_ACCELERATOR=tcg make run
```

For a headless boot smoke test using TCG:

```sh
make smoke-test-tcg
```

The runner expects the OVMF firmware at
`/usr/share/OVMF/OVMF_CODE_4M.fd` and
`/usr/share/OVMF/OVMF_VARS_4M.fd`.

Distributions that install these files elsewhere can override both paths:

```sh
OVMF_CODE=/path/to/OVMF_CODE_4M.fd \
OVMF_VARS_TEMPLATE=/path/to/OVMF_VARS_4M.fd \
QEMU_ACCELERATOR=tcg make run
```

### Run mochiOS with mBoot

To run the complete system with mBoot, build an mBoot disk image using the
QEMU system configuration:

```sh
make mboot-image \
    MBOOT_CONFIG="$PWD/mboot/config/qemu-system.toml"
```

The resulting disk image is written to:

```text
out/mochiOS.img
```

Create a writable copy of the OVMF variable store:

```sh
cp mboot/firmware/OVMF_VARS_4M.fd out/OVMF_VARS_4M.run.fd
```

Then start mBoot with QEMU and KVM:

```sh
qemu-system-x86_64 \
    -accel kvm \
    -cpu host \
    -machine q35,kernel-irqchip=split \
    -smp 2 \
    -m 1024 \
    -drive if=pflash,format=raw,readonly=on,file=mboot/firmware/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file=out/OVMF_VARS_4M.run.fd \
    -drive if=none,id=disk,format=raw,file=out/mochiOS.img \
    -device virtio-blk-pci,drive=disk,bootindex=1 \
    -device intel-iommu,intremap=on \
    -vga none \
    -device virtio-vga \
    -net none \
    -serial stdio \
    -monitor none \
    -no-reboot \
    -no-shutdown
```

In this configuration, mBoot starts the mochiOS System Domain together with
the mDriver Hardware Domain and manages CPU, memory, interrupts, IOMMU, and
device assignment between them.

Optional networking and TLS smoke tests additionally use `netcat-openbsd`
(`nc`) and Python 3.


## Status

mochiOS is under active development.

It is not yet intended for everyday use, and unexpected failures or data loss may occur.

Use a virtual machine or dedicated test machine when experimenting with it.

## Contributing

mochiOS is open source and contributions are welcome.

See [contributing.md](./contributing.md) before submitting changes.

<small>Copyright © 2026 mochiOS team.</small>
