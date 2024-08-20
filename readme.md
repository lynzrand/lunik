# Lunik

Lunik is a MoonBit toolchain multiplexer.

## Migrating from regular MoonBit toolchain

```shell
mkdir -p ~/.moon/lunik/toolchain/stable/
cp ~/.moon/bin/* ~/.moon/lunik/toolchain/stable/
cat >~/.moon/lunik.json <<EOF
{
  "toolchain": {
    "stable": {}
  },
  "default": "stable"
}
EOF
lunik link ~/.moon/bin/ moon moonc mooninfo moon_cove_report moonrun moondoc lunik -f
```

## Running

Symlink the Lunik executable with other names, and Lunik will spawn the correct version of the corresponding tool.

If you want to specify the toolchain to use, add `+<toolchain_name>` in the place of the first argument, like `moon +dev build`.

## Specifying new toolchains

A toolchain is represented by an object in `$.toolchain`.

Schema:

```ts
interface Toolchain {
    /** 
     * Fallback toolchain name. Queries that toolchain if the specified
     * tool does not exist in the current one.
     */
    fallback?: string  

    /**
     * Directory to find the tools. Defaults to `~/.moon/lunik/toolchain/<name>/`
     */
    root_path?: string

    /**
     * Override specific tools' paths.
     */
    override?: Map<string, string>
}
```

Example `lunik.json`:

```json
{
  "toolchain": {
    "stable": {},
    "dev": {
      "fallback": "stable",
      "override": {
        "moon": "/home/rynco/.cargo/bin/moon"
      }
    }
  },
  "default": "stable"
}
```

