# i18next typescript types generator

A Zig-based CLI tool that generates TypeScript type definitions from JSON translation files.

## Features

- Parses JSON translation files
- Supports ICU message format
- Generates TypeScript interfaces for type-safe translations
- Handles nested directory structures

## Prerequisites

- [Zig](https://ziglang.org/) (latest version recommended)

## Usage

```bash
typed-key <locales_dir> <output_dir>
```

## Building

To build the project:

```bash
zig build
```

## Contributing

This is an experimental project. Feel free to fork, modify, and use it as a learning resource for Zig development.

## License

MIT
