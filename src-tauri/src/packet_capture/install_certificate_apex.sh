#!/system/bin/sh
# Android 14+: /apex is private to each mount namespace. Keep each namespace's
# existing roots, and update Zygote before checking the running capture/game apps.
set -eu
source_cert=__CERT_PATH__
trust_dir=/apex/com.android.conscrypt/cacerts
staging=/data/local/tmp/mash-capture-ca-__SESSION_ID__
expected=__SOURCE_SHA256__
hash=__SUBJECT_HASH__
fail() { echo "$1" >&2; exit 1; }
test "$(id -u)" = 0 || fail '未取得 root 权限'
command -v nsenter >/dev/null || fail 'Android 14+ 证书安装需要设备提供 nsenter 命令'
verify_source() {
  test "$(sha256sum "$source_cert" | cut -d ' ' -f 1)" = "$expected" || fail 'Reqable 证书已变化，请重试'
}
has_certificate() {
  for cert in "$1/$hash".*; do
    if cmp -s "$source_cert" "$cert"; then return 0; fi
  done
  return 1
}
namespace() { readlink "/proc/$1/ns/mnt"; }
enter() { nsenter --mount="/proc/$1/ns/mnt" -- /system/bin/sh -c "$2"; }
verify_package() {
  package_pids=$(pidof "$1") || return 1
  for package_pid in $package_pids; do
    has_certificate "/proc/$package_pid/root$trust_dir" || return 1
  done
}
verify_source
zygotes="$(pidof zygote 2>/dev/null || true) $(pidof zygote64 2>/dev/null || true)"
test -n "$(echo "$zygotes" | tr -d ' ')" || fail '未找到 Zygote，无法配置 Android 14+ 系统证书'
reqable=$(pidof com.reqable.android) || fail 'Reqable 未运行，请重新打开后重试'
game=$(pidof com.bilibili.fatego 2>/dev/null || true)

# Preflight all namespaces before creating any mounts. Multiple processes can
# share one namespace; mounting it twice would make rollback remove the wrong layer.
namespaces=' '
targets=''
for pid in $zygotes $reqable $game; do
  ns=$(namespace "$pid") || fail '应用进程已变化，请重试环境配置'
  case "$namespaces" in *" $ns "*) continue ;; esac
  enter "$pid" "test -d '$trust_dir'" || fail '无法访问应用的 APEX 证书目录，请确认 root 允许 nsenter'
  test -n "$(ls "/proc/$pid/root$trust_dir")" || fail '应用的 APEX 证书目录为空，未修改证书'
  namespaces="$namespaces$ns "
  targets="$targets $pid"
done

mounted=''
pending=''
complete=0
cleanup() {
  if test "$complete" = 0; then
    rollback_failed=0
    for pid in $mounted; do
      # Never unmount from a reused PID. If the representative exited, another
      # process may still hold its namespace, so we cannot confirm rollback.
      current=$(namespace "$pid" 2>/dev/null || true)
      saved=$(cat "$staging/$pid.namespace")
      if test "$current" = "$saved"; then
        enter "$pid" "umount '$trust_dir'" || rollback_failed=1
      else
        rollback_failed=1
      fi
    done
    # Apps can be forked from Zygote while the transaction is running. Undo
    # inherited mounts too, but only if they still reference our backing inode.
    for process in /proc/[0-9]*; do
      for mounted_pid in $mounted; do
        if test "$process/root$trust_dir" -ef "$staging/$mounted_pid"; then
          child_pid=${process##*/}
          enter "$child_pid" "test '$trust_dir' -ef '$staging/$mounted_pid' && umount '$trust_dir'" || rollback_failed=1
        fi
      done
    done
    if test "$rollback_failed" = 1; then
      echo "证书安装失败，未能确认回滚，保留备份：${staging}；请重启设备后重试" >&2
      return 1
    fi
  fi
  # Keep backing files after any mount, even after successful rollback: a
  # concurrent fork may still hold an inherited mount outside our snapshot.
  if test -z "$mounted"; then rm -rf "$staging"; fi
}
mkdir -m 0700 "$staging"
trap cleanup EXIT
for pid in $targets; do
  namespace "$pid" > "$staging/$pid.namespace"
  if has_certificate "/proc/$pid/root$trust_dir"; then continue; fi
  stage=$staging/$pid
  mkdir -m 0755 "$stage"
  cp -a "/proc/$pid/root$trust_dir/." "$stage/"
  i=0
  while test -e "$stage/$hash.$i"; do
    i=$((i + 1))
    test "$i" -lt 1000 || fail '系统证书哈希目录异常'
  done
  cp "$source_cert" "$stage/$hash.$i"
  test "$(sha256sum "$stage/$hash.$i" | cut -d ' ' -f 1)" = "$expected" || fail '证书校验失败'
  chown -R 0:0 "$stage"
  chmod 0755 "$stage"
  chmod 0644 "$stage/"*
  if test "$(getenforce)" != Disabled; then
    chcon -R u:object_r:system_file:s0 "$stage"
  fi
  pending="$pending $pid"
done
verify_source
for pid in $pending; do
  test "$(namespace "$pid")" = "$(cat "$staging/$pid.namespace")" || fail '应用进程已变化，请重试环境配置'
  enter "$pid" "mount -o bind '$staging/$pid' '$trust_dir'" || fail 'APEX 证书挂载失败，正在撤销本次安装'
  mounted="$pid $mounted"
  has_certificate "/proc/$pid/root$trust_dir" || fail 'APEX 证书回读校验失败，正在撤销本次安装'
done
# Re-read live PIDs to catch apps launched during installation. Future apps
# inherit the Zygote mounts; existing apps must also see the certificate.
for pid in $zygotes; do
  has_certificate "/proc/$pid/root$trust_dir" || fail 'Zygote 证书校验失败，正在撤销本次安装'
done
verify_package com.reqable.android || fail '证书未对 Reqable 生效，请重试环境配置'
if pidof com.bilibili.fatego >/dev/null; then
  verify_package com.bilibili.fatego || fail '证书未对国服 FGO 生效，请重新进入游戏后重试环境配置'
fi
verify_source
complete=1
if test -z "$mounted"; then
  echo MASH_CA_ALREADY_INSTALLED
else
  echo MASH_CA_INSTALLED
fi
