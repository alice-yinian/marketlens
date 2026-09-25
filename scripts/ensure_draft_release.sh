#!/usr/bin/env bash
# 确保 tag `$GITHUB_REF_NAME` 下有**且仅有**一条草稿 Release。
#
# ## 为什么需要这个脚本
#
# 原先两个平台 job（windows / android）各自用
#
#     gh release view "$TAG" >/dev/null 2>&1 || gh release create "$TAG" --draft ...
#
# 注释写着「并发安全」，但事实相反：`gh release create` 在 release 已存在时
# **既不报错、也不复用，而是再建一条同名记录**（实测确认，且返回 exit 0）。
# 两个 job 同时走到这一步就会各建一条。
#
# 后果不是「报错」这种干脆的失败，而是更隐蔽的错乱：
#   * `gh release view <tag>` 在多条同名记录里只返回其中一条；
#   * `gh release upload <tag>` 未必传到同一条；
#   于是资产被拆散到不同记录上，release job 校验时看到「资产不全」而拒绝发布，
#   或者发布出一个空壳 Release。
#
# ## 另一条必须知道的约束
#
# `gh release view/edit/upload` **只接受 tag，不接受 release id**
# （用 id 会报 `release not found`，实测确认）。所以没法「按 id 精准操作」
# 绕开重复问题——只能先把「该 tag 下恰好一条」这个不变量立住。
#
# 结构上，创建动作已收敛到单独的 `draft` job（windows/android 都 `needs` 它），
# 从根上消除并发创建。本脚本作为该 job 的实现，并保留对残留脏状态的检查
# （例如上一次失败的运行留下了重复项）。
#
# ## 环境
#   GITHUB_REF_NAME     必填，tag 名（如 v0.1.0）
#   GITHUB_REPOSITORY   必填，owner/repo
#   GH_TOKEN            必填，需要 contents: write

set -euo pipefail

: "${GITHUB_REF_NAME:?需要 GITHUB_REF_NAME（tag 名）}"
: "${GITHUB_REPOSITORY:?需要 GITHUB_REPOSITORY（owner/repo）}"

# 该 tag 下所有 release 的 id（含草稿——草稿对写权限 token 可见）。
# `--paginate` 让它跨页拼接，避免 >100 条 release 时漏数。
list_release_ids() {
  gh api --paginate "repos/$GITHUB_REPOSITORY/releases?per_page=100" \
    --jq ".[] | select(.tag_name == \"$GITHUB_REF_NAME\") | .id"
}

# 数一个可能为空的换行分隔列表
count_ids() {
  if [ -z "${1:-}" ]; then
    echo 0
  else
    printf '%s\n' "$1" | wc -l | tr -d ' '
  fi
}

# 列表接口在 create 之后可能短暂看不到新记录（实测遇到过返回 0 的情况），
# 所以计数带重试。`gh release view` 则是即时的，用它判断「存在性」。
count_ids_with_retry() {
  local attempt ids
  for attempt in 1 2 3 4 5; do
    ids="$(list_release_ids)"
    if [ -n "$ids" ]; then
      count_ids "$ids"
      return 0
    fi
    sleep 2
  done
  echo 0
}

EXISTS=false
if gh release view "$GITHUB_REF_NAME" >/dev/null 2>&1; then
  EXISTS=true
  echo "tag $GITHUB_REF_NAME 下已有 Release，复用。" >&2
else
  echo "tag $GITHUB_REF_NAME 下没有 Release，创建草稿……" >&2
  # 不加 --generate-notes：此时还没有可用的说明内容，release job 会覆写。
  gh release create "$GITHUB_REF_NAME" --draft \
    --title "$GITHUB_REF_NAME" --notes "构建中……" >/dev/null

  # 确认创建真的生效（view 是即时的，不用重试列表接口）
  if ! gh release view "$GITHUB_REF_NAME" >/dev/null 2>&1; then
    echo "::error::创建草稿后仍查不到 tag $GITHUB_REF_NAME 的 Release，放弃。" >&2
    exit 1
  fi
fi

# 重复项检查。多于一条时明确失败：继续下去只会把资产拆散到不同记录，
# 产出一个「看起来构建成功、实际缺文件」的 Release——宁可红。
COUNT="$(count_ids_with_retry)"
if [ "$COUNT" -gt 1 ]; then
  {
    echo "::error::tag $GITHUB_REF_NAME 下有 $COUNT 条 Release（应为 1 条）。"
    echo "这通常意味着并发的 release create 各建了一条，或上次失败的运行留下了残留。"
    echo "请先清理多余的草稿，再重跑："
    echo "  gh release list --repo $GITHUB_REPOSITORY"
    echo "  gh api -X DELETE repos/$GITHUB_REPOSITORY/releases/<多余的 id>"
  } >&2
  exit 1
fi

if [ "$COUNT" -eq 0 ]; then
  # view 说存在、列表却数不到：极端的分页/一致性窗口。此时无法确认唯一性，
  # 但至少已知存在一条（view 成功了），继续比卡住更有价值。
  echo "::warning::列表接口暂时看不到 tag $GITHUB_REF_NAME 的 Release，跳过重复检查。" >&2
fi

echo "草稿 Release 就绪：tag=$GITHUB_REF_NAME（exists=$EXISTS, count=$COUNT）" >&2
