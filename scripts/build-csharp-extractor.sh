#!/usr/bin/env sh
set -eu

usage() {
  echo "usage: $0 --bin-dir <dir> --libexec-dir <dir> [--relative-libexec <path>]" >&2
}

bin_dir=
libexec_dir=
relative_libexec=

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin-dir)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      bin_dir=$2
      shift 2
      ;;
    --libexec-dir)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      libexec_dir=$2
      shift 2
      ;;
    --relative-libexec)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      relative_libexec=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

[ -n "$bin_dir" ] || { usage; exit 2; }
[ -n "$libexec_dir" ] || { usage; exit 2; }

mkdir -p "$bin_dir" "$libexec_dir"

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

dotnet publish "$repo_root/tools/mdlr-extract-csharp/mdlr-extract-csharp.csproj" \
  -c Release \
  --self-contained false \
  -p:UseAppHost=false \
  -o "$libexec_dir" \
  --nologo \
  -v q

launcher="$bin_dir/mdlr-extract-csharp"
if [ -n "$relative_libexec" ]; then
  libexec_expr='$(dirname -- "$resolved")/'"$relative_libexec"
else
  libexec_abs=$(CDPATH= cd -- "$libexec_dir" && pwd)
  libexec_expr=$libexec_abs
fi

cat > "$launcher" <<EOF
#!/usr/bin/env sh
set -eu

target=\$0
while [ -L "\$target" ]; do
  dir=\$(CDPATH= cd -- "\$(dirname -- "\$target")" && pwd)
  link=\$(readlink "\$target")
  case "\$link" in
    /*) target=\$link ;;
    *) target=\$dir/\$link ;;
  esac
done
resolved=\$(CDPATH= cd -- "\$(dirname -- "\$target")" && pwd)/\$(basename -- "\$target")

if ! command -v dotnet >/dev/null 2>&1; then
  echo "mdlr-extract-csharp: dotnet was not found on PATH; install the .NET SDK to enable C# extraction" >&2
  exit 1
fi

exec dotnet "$libexec_expr/mdlr-extract-csharp.dll" "\$@"
EOF
chmod +x "$launcher"
