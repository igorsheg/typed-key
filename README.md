# i18next LSP

![TypedKey Logo](https://github.com/igorsheg/typed-key/blob/main/res/icon.png?raw=true)

A Language Server Protocol (LSP) implementation for i18next, providing intelligent features for your internationalization needs.

## Features

- Smart autocompletion for translation keys
- Hover information with translation previews
- Type checking for translation parameters
- Compatible with Neovim (native LSP) and Visual Studio Code

## Installation

### Visual Studio Code

Install the extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=igorsheg.typed-key).

### Neovim

Add the following to your Neovim configuration:

```lua
require('lspconfig').typedkey.setup{}
```

## Usage

Once installed, the LSP will automatically activate for supported file types, providing enhanced i18next support.

## Configuration

In Visual Studio Code, you can configure the extension through the following settings:

- `typedkey.path`: Path to the `typed-key` binary. If empty, the bundled binary will be used.
- `typedkey.translationsDir`: Directory to search for translation files. Default: `"src/assets/locales"`
- `typedkey.logLevel`: Logging level of the language server. Options: `"off"`, `"error"`, `"warn"`, `"info"`, `"debug"`, `"trace"`. Default: `"warn"`
- `typedkey.trace.server`: Traces the communication between VS Code and the language server. Options: `"off"`, `"messages"`, `"verbose"`. Default: `"off"`

For Neovim users, please refer to the LSP configuration documentation for setup options.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License.
