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

```sh
git clone https://github.com/mochiOS/mochiOS.git
cd mochiOS

repo init \
    -u https://github.com/mochiOS/mochiOS.git \
    -b master

repo sync
make build
```

For build requirements and detailed instructions, see the developer documentation.

## Status

mochiOS is under active development.

It is not yet intended for everyday use, and unexpected failures or data loss may occur.

Use a virtual machine or dedicated test machine when experimenting with it.

## Contributing

mochiOS is open source and contributions are welcome.

See [contributing.md](./contributing.md) before submitting changes.

<small>Copyright © 2026 mochiOS team.</small>
