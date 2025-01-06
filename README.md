Basmati is a cli command like utility to stream archives up and down, to and from AWS Glacier, AWS's cold storage offering. Cold storage means that when you want your files you have to send in a request to be fulfilled within 6 - 12 hours. The process is pretty tiresome as you need to dowload your archive within a certain time frame of having initiated the download job. Basmati makes it easy by showing your inventory in a TUI application, polling Glacier until the job is ready and completed and calculating all the annoying treehashes to successfuly upload archives.

### Environment and setup

The tool assumes your `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` are in your environment already. There is currently no way to pass those in as command line arguments.

### Packaging

Basmati is currently available as a crate or as Nix flake

```
# crate
cargo install basmati
```

```nix
# get the flake
{
  inputs = {
    ...
    basmati.url = "github:vhsconnect/basmati";

};

```

```nix
# add to your packages attribute set
{pkgs, inputs, ...} :{
    environment.systemPackages = [
        inputs.basmati.packages.${pkgs.system}.default
    ]

};


```

### USAGE

```
Usage: basmati [OPTIONS] [COMMAND]

Commands:
  create
  upload
  inventory
  download
  help       Print this message or the help of the given subcommand(s)

Options:
  -d, --debug...
  -h, --help      Print help
  -V, --version   Print version

```

#### create

Create a new vault

```
Usage: basmati create --vault-name <VAULT_NAME>

Options:
  -v, --vault-name <VAULT_NAME>
  -h, --help                     Print help
```

#### upload

Upload an archive or file

```

Usage: basmati upload --file-path <FILE_PATH> --vault-name <VAULT_NAME>

Options:
  -f, --file-path <FILE_PATH>
  -v, --vault-name <VAULT_NAME>
  -h, --help                     Print help
```

#### download

Download an archive by specifying a vault and path. You must have run `inventory` command first to download a ledger of your assets

```
Usage: basmati download --vault-name <VAULT_NAME> --output-as <OUTPUT_AS>

Options:
  -v, --vault-name <VAULT_NAME>
  -o, --output-as <OUTPUT_AS>
  -h, --help
```

#### inventory

Download an invenory list for a specific vault

```
Usage: basmati inventory --vault-name <VAULT_NAME> --desc <DESC>

Options:
  -v, --vault-name <VAULT_NAME>
  -d, --desc <DESC>
  -h, --help                     Print help
```

#### delete-archive

Delete an archive from a vault

```
Usage: basmati delete-archive --vault-name <VAULT_NAME>

Options:
  -v, --vault-name <VAULT_NAME>
  -h, --help                     Print help
```

#### list-vaults

List all vaults for current account

```
Usage: basmati list-vaults

Options:
  -h, --help  Print help
```

## TODO

- better sanitization for cache and TMP
- better input handling when in tui
- Add option to view inventory - really same as download screen - perhaps an alias?
- Add option for just finishing pending jobs without creating new jobs
- ensure output_as option works


