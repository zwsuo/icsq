# 发布到 AUR

AUR 只托管 `PKGBUILD` 和 `.SRCINFO`，**不托管源码**。所以顺序是：

1. 把 `icsq` 放到公开 Git 仓库并打 tag；
2. 在 AUR 仓库里放 PKGBUILD；
3. `makepkg --printsrcinfo` 生成 `.SRCINFO`；
4. push 到 `ssh://aur@aur.archlinux.org/icsq.git`。

包名 `icsq` 目前未被占用（2026-11 查询 AUR RPC 结果为空）。

## 1. 准备上游源码仓库

发布前必须补上：

- `LICENSE` 文件（MIT、Apache-2.0 等），并让 `PKGBUILD` 的 `license=` 与之一致；本项目已经使用 MIT；
- 提交 `Cargo.lock`（二进制项目应该锁定依赖版本，本项目的 `.gitignore` 已不再忽略它）；
- `README.md`。

假设最终仓库根目录就是 `icsq/`（`Cargo.toml` 在根上）：

```bash
cd icsq
git init
git add .
git commit -m "icsq 1.1.2"
git tag -a v1.1.2 -m "icsq 1.1.2"
git remote add origin git@github.com:zwsuo/icsq.git
git push -u origin master --tags
```

GitHub 会自动提供 tag 压缩包：

```text
https://github.com/zwsuo/icsq/archive/refs/tags/v1.1.2.tar.gz
```

## 2. 填写 PKGBUILD

把 `packaging/PKGBUILD` 里的 `zwsuo`、`Maintainer`、`license` 改好。然后：

```bash
cd packaging
updpkgsums          # 下载 tag 压缩包并写入真实 sha256sums
makepkg -f          # 本地构建，产出 icsq-1.1.2-1-x86_64.pkg.tar.zst
makepkg --printsrcinfo > .SRCINFO
```

建议再检查：

```bash
sudo pacman -S --needed namcap devtools
namcap PKGBUILD
namcap icsq-1.1.2-1-x86_64.pkg.tar.zst
```

`makepkg` 要以普通用户运行，不要加 `sudo`。

## 3. 提交到 AUR

1. 注册账号：<https://aur.archlinux.org/register/>
2. 在 AUR 的 My Account 里填入 SSH 公钥（`~/.ssh/id_ed25519.pub`）；
3. clone 空仓库并推送：

```bash
git clone ssh://aur@aur.archlinux.org/icsq.git
cd icsq
cp /path/to/packaging/PKGBUILD .
cp /path/to/packaging/.SRCINFO .
git add PKGBUILD .SRCINFO
git commit -m "Add icsq 1.1.2"
git push
```

AUR 仓库的根目录只应有 `PKGBUILD`、`.SRCINFO`，以及可选的 `.install`。
构建产物、`src/`、`pkg/` 都不要提交。

## 4. 以后更新版本

```bash
# 源码仓库
git tag -a v0.1.1 -m "icsq 0.1.1"
git push origin master --tags

# AUR 仓库
# 修改 PKGBUILD 里的 pkgver=0.1.1
updpkgsums
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "icsq 0.1.1"
git push
```

只改打包脚本时递增 `pkgrel`；改上游版本时更新 `pkgver` 并把 `pkgrel` 重置为 1。

## 可选：`icsq-git`

如果想发布跟踪最新提交的包，另建一个 `icsq-git` 包，`source` 用：

```bash
source=("$pkgname::git+$url.git")
sha256sums=('SKIP')
```

并实现 `pkgver()`：

```bash
pkgver() {
  cd "$pkgname"
  printf 'r%s.%s' "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}
```

`icsq`（稳定版）和 `icsq-git` 可以同时存在，用
`provides=('icsq')`、`conflicts=('icsq')` 处理互斥关系。
