#!/bin/bash
# 在 Mac 上双击运行：打包邻传（Mac 通用版 + Windows 版，输出到 dist/），
# 然后把新的 邻传.app 安装到桌面“邻传”文件夹并启动。
# 也可以在终端运行：scripts/mac_build_install.command [安装目录]
cd "$(dirname "$0")/.." || exit 1
ROOT="$(pwd)"
DEST="${1:-$HOME/Desktop/邻传}"
LOG="$ROOT/dist/build-log.txt"
mkdir -p "$ROOT/dist"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
{
  echo "== $(date '+%F %T') 开始打包"
  sw_vers 2>/dev/null; uname -m
  rustc --version; cargo --version; rustup --version 2>/dev/null | head -1
  xcode-select -p
} 2>&1 | tee "$LOG"
python3 scripts/build_release.py 2>&1 | tee -a "$LOG"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ]; then
  echo "== $(date '+%F %T') 打包失败（退出码 $status），旧版未改动" | tee -a "$LOG"
  exit "$status"
fi
VERSION=$(python3 -c "import re;print(re.search(r\"VERSION='([^']+)'\",open('scripts/build_release.py').read()).group(1))")
ZIP="$ROOT/dist/LanTransfer-$VERSION-macOS-universal.zip"
{
  echo "== 安装到 $DEST"
  pkill -x lan-transfer && sleep 1
  TMP=$(mktemp -d)
  ditto -x -k "$ZIP" "$TMP" || exit 1
  mkdir -p "$DEST"
  if [ -d "$DEST/邻传.app" ]; then
    OLD=$(defaults read "$DEST/邻传.app/Contents/Info" CFBundleShortVersionString 2>/dev/null)
    BACKUP="$DEST/邻传 ${OLD:-旧版} 旧版（可删除）.app"
    if [ "$OLD" = "$VERSION" ] || [ -e "$BACKUP" ]; then
      rm -rf "$DEST/邻传.app"
    else
      mv "$DEST/邻传.app" "$BACKUP" && echo "旧版已改名为：$BACKUP"
    fi
  fi
  ditto "$TMP/邻传.app" "$DEST/邻传.app"
  cp "$TMP/使用说明.txt" "$TMP/FONT-LICENSE.txt" "$TMP/THIRD-PARTY-NOTICES.txt" "$DEST/"
  rm -rf "$TMP"
  codesign --verify --deep --strict "$DEST/邻传.app" && echo "签名校验通过"
  open "$DEST/邻传.app"
  echo "== $(date '+%F %T') 完成：已启动 $DEST/邻传.app"
} 2>&1 | tee -a "$LOG"
