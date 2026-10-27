#!/system/bin/sh
# Called only after the public CA has been parsed and hashed by Rust.
set -eu
source_cert=__CERT_PATH__
trust_dir=/system/etc/security/cacerts
staging=/data/local/tmp/mash-capture-ca-__SESSION_ID__
expected=__SOURCE_SHA256__
hash=__SUBJECT_HASH__
fail() { echo "$1" >&2; exit 1; }
test "$(id -u)" = 0 || fail '未取得 root 权限'
test -d "$trust_dir" || fail '未找到系统证书目录'
verify_source() {
  test "$(sha256sum "$source_cert" | cut -d ' ' -f 1)" = "$expected" || fail 'Reqable 证书已变化，请重试'
}
verify_app() {
  pids=$(pidof com.reqable.android) || return 1
  for pid in $pids; do
    cmp -s "$source_cert" "/proc/$pid/root$trust_dir/$name" || return 1
  done
}
verify_source
i=0
while test -e "$trust_dir/$hash.$i"; do
  name=$hash.$i
  if cmp -s "$source_cert" "$trust_dir/$name"; then
    verify_app || fail 'Reqable 尚未识别系统证书，请重新打开 Reqable 后重试'
    echo MASH_CA_ALREADY_INSTALLED
    exit 0
  fi
  i=$((i + 1))
  test "$i" -lt 1000 || fail '系统证书哈希目录异常'
done
name=$hash.$i
mounted=0
complete=0
cleanup() {
  if test "$complete" = 0; then
    if test "$mounted" = 1; then
      umount "$trust_dir" || return 1
    fi
    rm -rf "$staging"
  fi
}
# mkdir must succeed before registering cleanup; never remove an existing path.
mkdir -m 0700 "$staging"
trap cleanup EXIT
cp -a "$trust_dir/." "$staging/"
cp "$source_cert" "$staging/$name"
test "$(sha256sum "$staging/$name" | cut -d ' ' -f 1)" = "$expected" || fail '证书校验失败'
chown -R 0:0 "$staging"
chmod 0755 "$staging"
chmod 0644 "$staging/$name"
if test "$(getenforce)" != Disabled; then
  chcon -R u:object_r:system_file:s0 "$staging"
fi
verify_source
mount -o bind "$staging" "$trust_dir"
mounted=1
cmp -s "$source_cert" "$trust_dir/$name" || fail '系统证书回读校验失败'
verify_app || fail '证书未对 Reqable 生效，已取消本次安装'
complete=1
echo MASH_CA_INSTALLED
