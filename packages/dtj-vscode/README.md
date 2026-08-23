# DTJ VS Code Integration

This is an optional integration for VS Code that provides .dtj file support —
completely independent of Doc Hub paid packs.

## Features

- View .dtj session metadata in the editor
- Browse event chunks and dictionary entries
- Tail last N events
- Search events by fields

## Build

```bash
cd packages/dtj-vscode
npm install
npm run compile
```
- Search events by fields
- No paid licensing required

## Installation

```bash
# From VS Code Extensions
# Search for "DTJ Session Explorer" or install from VSIX

# Or build from source
cd packages/dtj-vscode
npm install
npm run compile
code --install-ext package/dtj-vscode.vsix
```

## Independence Note

This VS Code integration was extracted from `dochub-pack-dtj/dtj-vscode/`
and has been refactored to:
- Remove all Doc Hub branding and licensing checks
- Use only the independent DTJ byte format specification
- Function without any Doc Hub runtime or paid pack
- Support stdio-based MCP boundary

## License

MIT OR Apache-2.0