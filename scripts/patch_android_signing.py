#!/usr/bin/env python3
"""给 Tauri 生成的 Android 工程打上 release 签名配置。

## 为什么需要脚本而不是直接改文件

`src-tauri/gen/android/` **不入库**（`.gitignore`），每次 CI 都重新生成，
所以签名配置没法「提交一次就完事」——必须在 `tauri android init` 之后自动打补丁。

步骤来自 Tauri 官方文档（Android Code Signing）：
  1. `import java.io.FileInputStream`
  2. 在 `buildTypes` 之前插入 `signingConfigs { create("release") { ... } }`
  3. 在 `buildTypes.getByName("release")` 里启用它

## 关键性质：锚点找不到就**报错退出**

静默跳过会让 CI 产出一个**未签名**的 APK，而它看起来完全正常——
直到有人拿去安装才发现装不上。宁可让 CI 红，也不要产出这种产物。
"""

from __future__ import annotations

import sys
from pathlib import Path

# 两个 import 都必须有：`FileInputStream` 用来读 keystore.properties，
# `Properties` 用来解析它。少任何一个 Kotlin 都编译不过，
# 而错误会以「Gradle 编译失败」的形式出现，很难联想到是脚本漏了一行。
REQUIRED_IMPORTS = ("import java.io.FileInputStream", "import java.util.Properties")

SIGNING_CONFIG = '''
signingConfigs {
    create("release") {
        val keystorePropertiesFile = rootProject.file("keystore.properties")
        val keystoreProperties = Properties()
        if (keystorePropertiesFile.exists()) {
            keystoreProperties.load(FileInputStream(keystorePropertiesFile))
        }
        keyAlias = keystoreProperties["keyAlias"] as String
        keyPassword = keystoreProperties["password"] as String
        storeFile = file(keystoreProperties["storeFile"] as String)
        storePassword = keystoreProperties["password"] as String
    }
}
'''

RELEASE_SIGNING_LINE = "        signingConfig = signingConfigs.getByName(\"release\")\n"

# 必须存在、否则说明 Tauri 换了模板结构
REQUIRED_ANCHORS = ("buildTypes", 'getByName("release")')


class PatchError(RuntimeError):
    pass


def patch(source: str) -> str:
    """返回打好补丁的内容。锚点缺失时抛 PatchError。"""
    for anchor in REQUIRED_ANCHORS:
        if anchor not in source:
            raise PatchError(
                f"在生成的 build.gradle.kts 里找不到锚点 {anchor!r}。"
                "这通常意味着 Tauri 换了 Android 模板结构——"
                "请对照官方文档重新确认签名补丁的位置，不要直接跳过。"
            )

    if 'signingConfigs.getByName("release")' in source:
        # 幂等：重复执行不该插两次
        return source

    output = source

    # 1. 补 import
    for required in REQUIRED_IMPORTS:
        if required in output:
            continue
        lines = output.splitlines(keepends=True)
        last_import = 0
        for index, line in enumerate(lines):
            if line.startswith("import "):
                last_import = index + 1
        lines.insert(last_import, required + "\n")
        output = "".join(lines)

    # 2. 在 buildTypes 之前插入 signingConfigs
    marker = "buildTypes {"
    index = output.index(marker)
    output = output[:index] + SIGNING_CONFIG.strip() + "\n\n" + output[index:]

    # 3. 在 release 构建类型里启用
    release = 'getByName("release") {'
    index = output.index(release) + len(release)
    output = output[:index] + "\n" + RELEASE_SIGNING_LINE.rstrip("\n") + output[index:]

    return output


# 贴近 Tauri v2 生成物的结构：已有 Properties import，没有 FileInputStream。
# 真实生成物没法在开发机拿到（需要 Android SDK），所以这里用它的结构做自测——
# 自测能覆盖的是**补丁逻辑**，覆盖不了「Tauri 是否换了模板」，
# 后者由锚点检查在 CI 里兜住。
SELF_TEST_SAMPLE = '''import java.util.Properties

plugins {
    id("com.android.application")
}

android {
    namespace = "com.marketlens.app"

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
        }
        getByName("debug") {
            isDebuggable = true
        }
    }
}
'''


def self_test() -> int:
    """不依赖测试框架的自测：`python3 scripts/patch_android_signing.py --self-test`。"""
    failures: list[str] = []

    patched = patch(SELF_TEST_SAMPLE)
    for needle in REQUIRED_IMPORTS + (
        'create("release")',
        'signingConfig = signingConfigs.getByName("release")',
    ):
        if needle not in patched:
            failures.append(f"补丁缺少：{needle}")

    if patched.index("signingConfigs {") > patched.index("buildTypes {"):
        failures.append("signingConfigs 必须在 buildTypes 之前")

    if "isDebuggable = true" not in patched:
        failures.append("debug 块被改动了")

    if patch(patched) != patched:
        failures.append("重复执行改变了内容（不幂等）")

    if patched.count("import java.util.Properties") != 1:
        failures.append("Properties import 重复了")

    try:
        patch("plugins { }\n// 没有 buildTypes\n")
        failures.append("锚点缺失时必须抛错，而不是静默产出未签名 APK")
    except PatchError:
        pass

    for failure in failures:
        print(f"失败：{failure}", file=sys.stderr)
    if not failures:
        print("自测通过")
    return 1 if failures else 0


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        return self_test()

    if len(sys.argv) != 2:
        print(
            "用法: patch_android_signing.py <path/to/app/build.gradle.kts> | --self-test",
            file=sys.stderr,
        )
        return 2

    path = Path(sys.argv[1])
    if not path.exists():
        print(f"找不到文件：{path}", file=sys.stderr)
        return 1

    original = path.read_text()
    try:
        patched = patch(original)
    except PatchError as err:
        print(f"::error::{err}", file=sys.stderr)
        return 1

    if patched == original:
        print("签名配置已存在，跳过")
        return 0

    path.write_text(patched)
    print(f"已为 {path} 打上 release 签名配置")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
