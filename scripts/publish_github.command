#!/bin/bash
# 双击运行：重新打包（二进制中不含本机路径），然后把源码推送到 GitHub
# 仓库 lan-transfer（默认私有），并把 Mac / Windows 安装包发布为 Release。
# 使用本机 GitHub CLI（gh）的登录，不需要在任何地方填写令牌。
cd "$(dirname "$0")/.." || exit 1
ROOT="$(pwd)"; REPO="${REPO:-lan-transfer}"; VISIBILITY="${VISIBILITY:-private}"
mkdir -p dist; LOG="$ROOT/dist/publish-log.txt"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
exec > >(tee "$LOG") 2>&1
step() { echo; echo "== $(date '+%T') $*"; }
fail() { echo "== 失败：$*"; exit 1; }

step "检查 GitHub CLI"
command -v gh >/dev/null || fail "没有安装 gh。请先安装（brew install gh），再运行 gh auth login 登录后重试。"
gh auth status || fail "gh 尚未登录。请在终端运行 gh auth login 登录后重试。"
LOGIN=$(gh api user --jq .login) || fail "无法读取 GitHub 账号"
ID=$(gh api user --jq .id)
echo "账号：$LOGIN"

step "重新打包（去除本机路径）"
python3 scripts/build_release.py || fail "打包失败"
VERSION=$(python3 -c "import re;print(re.search(r\"VERSION='([^']+)'\",open('scripts/build_release.py').read()).group(1))")
ASSETS=(dist/LanTransfer-$VERSION-macOS-universal.zip dist/LanTransfer-$VERSION-macOS-universal.dmg dist/LanTransfer-$VERSION-Windows-x64.zip dist/SHA256SUMS.txt)
for f in "${ASSETS[@]}"; do [ -f "$f" ] || fail "缺少 $f"; done
T=$(mktemp -d); ditto -x -k "${ASSETS[0]}" "$T/mac"; ditto -x -k "${ASSETS[2]}" "$T/win"
if grep -a -r -l -F "$HOME" "$T" ; then fail "安装包中仍含本机路径"; fi
rm -rf "$T"; echo "安装包检查通过：不含本机路径"

step "准备 Git 仓库"
[ -d .git ] || git init -q -b main
git config user.name "$LOGIN"
git config user.email "$ID+$LOGIN@users.noreply.github.com"
git add -A
BAD=$(git ls-files --cached | grep -E '^(dist|target|localsend|\.build-tools|备份-)' || true)
[ -z "$BAD" ] || fail "以下文件不应上传：$BAD"
if git grep -n -I -F "$HOME" -- . ':!Cargo.lock'; then fail "源码中含本机路径"; fi
git diff --cached --quiet || git commit -q -m "邻传 $VERSION：免密钥配对、整批确认、文件夹与文字、界面重做"
git log --oneline | head -3

step "推送到 GitHub：$LOGIN/$REPO（$VISIBILITY）"
if ! gh repo view "$LOGIN/$REPO" >/dev/null 2>&1; then
  gh repo create "$REPO" --"$VISIBILITY" --description "邻传 · Mac / Windows 局域网文件互传" || fail "创建仓库失败"
fi
git remote get-url origin >/dev/null 2>&1 || git remote add origin "https://github.com/$LOGIN/$REPO.git"
gh auth setup-git
git push -u origin main || fail "推送失败"

step "发布 Release v$VERSION"
NOTES=$(mktemp); cat > "$NOTES" <<NOTES_END
邻传 $VERSION

- 不再需要连接密钥：打开即可发现同一网络的电脑，首次由接收方确认，可勾选“信任此设备”以后自动接收
- 一批文件只确认一次；支持发送文件夹和文字
- 界面重新设计，浅色 / 深色跟随系统

下载：Mac 用 macOS-universal（zip 或 dmg，Apple Silicon 与 Intel 通用）；Windows 用 Windows-x64.zip，解压后运行 LanTransfer.exe。
与 0.2 不兼容，两台电脑都需更新。未经 Apple 公证 / Windows 签名，首次打开可能需要确认。
NOTES_END
if gh release view "v$VERSION" -R "$LOGIN/$REPO" >/dev/null 2>&1; then
  gh release upload "v$VERSION" "${ASSETS[@]}" --clobber -R "$LOGIN/$REPO" || fail "上传安装包失败"
else
  gh release create "v$VERSION" "${ASSETS[@]}" -R "$LOGIN/$REPO" --title "邻传 $VERSION" --notes-file "$NOTES" || fail "创建 Release 失败"
fi
step "完成：https://github.com/$LOGIN/$REPO"
