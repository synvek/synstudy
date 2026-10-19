# 以「桌面本地构建」的方式编译 / 运行带截图翻译功能的 synstudy（Windows，Vulkan 后端）。
#
# 这是 macOS 端 scripts/check_translate.sh 的 Windows 对应脚本。开启翻译功能会拉起
# 一批需要本机 C/C++ 工具链的依赖：llama.cpp(ggml)、MNN、bindgen(cbindgen) 等，
# 其中 llama.cpp 的 Vulkan 后端还需要 Vulkan SDK。直接 `cargo build --features
# translate-vulkan` 在普通 PowerShell 里通常会在三处失败：
#
#   1) 找不到 MSVC 的 cl.exe / link.exe —— 它们只在「Developer PowerShell for VS」
#      （即跑过 VsDevCmd.bat）的环境里才在 PATH 上。
#   2) ggml-vulkan 的 build.rs 读取 `VULKAN_SDK` 环境变量定位头文件与 vulkan-1.lib，
#      并把 `%VULKAN_SDK%\Bin\glslc` 用于把 GLSL 着色器编译成 SPIR-V；没设就报
#      "Please install Vulkan SDK and ensure that VULKAN_SDK env variable is set"。
#   3) mnn-sys / llama-cpp-sys 的 build.rs 用 cmake crate 编译 C/C++，它会用
#      vswhere -latest 探测到机器上最新的 VS。若装了 VS 2026(v18) 而 CMake < 4.2，
#      就会拼出 `-G "Visual Studio 18 2026"` 而 CMake 报 "Could not create named
#      generator Visual Studio 18 2026"（该生成器 CMake 4.2 才加入）。本脚本会把
#      生成器锚定到本机 CMake 认识的那个 VS（如 2022），没有匹配项则退回 Ninja。
#   3) LLVM/libclang：bindgen 需要 clang 驱动与 libclang 同源；AMD ROCm 会抢 PATH，
#      脚本自动找完整 LLVM 前置对齐（否则 mnn-sys 报 __builtin_ia32_* 未声明）。
#   4) 仓库路径偏深时，llama.cpp Vulkan 子项目（CMake ExternalProject 单独
#      configure）的中间文件路径会超 CMake 250 字符安全上限 / Win32 260 限制，
#      报 rules.ninja 找不到或 MSB8066。本脚本检测到偏深时会自动把构建目录
#      搬到短 base（CARGO_TARGET_DIR=C:\dst，可用 -TargetDir/-KeepTargetDir 干预）。
#
# 本脚本把这些一次性配好：自动定位并导入 VS 开发者环境（vcvars）、探测 Vulkan SDK、
# 把 SDK\Bin 加进 PATH、用系统 git 拉取 git 依赖，再调用 cargo / dx。
#
# 用法
# ----
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1              # cargo check  --features translate-vulkan
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 build        # cargo build   --features translate-vulkan
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 build -Release
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 run          # cargo run     --features translate-vulkan
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 clippy
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 bundle       # dx bundle --features translate-vulkan（出 .exe 安装包）
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 pin         # 把传递依赖 mnn 固定到 v1.4.2 兼容 rev（首次构建报 E0106/E0308 时跑）
#   powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 env          # 只打印配置好的工具链环境，便于排查
#   ... check_translate.ps1 bundle -Release -CMakeGenerator 'Visual Studio 17 2022'   # 手动锁定 CMake 生成器（留空=自动）
#   ... check_translate.ps1 build -CMakeGenerator Ninja                               # 强制用 Ninja 生成器（需 ninja.exe 在 PATH）
#   ... check_translate.ps1 bundle -Release -KeepTargetDir                            # 不搬家构建目录（保持项目内 target\）
#   ... check_translate.ps1 bundle -Release -TargetDir D:\bt                          # 指定短 base（产物安装包也输出到那里）
#
# 额外参数会原样透传给底层命令，例如：
#   ... check_translate.ps1 check --offline
#
# CUDA 备选：本机是 NVIDIA 显卡且装了 CUDA Toolkit 时，把 -Feature 换成 translate-cuda
#   ... check_translate.ps1 build -Feature translate-cuda
#
# 网络提示：rust-paddle-ocr / mnn 均为 GitHub git 源，且其 build.rs(cbindgen) 会
# 非离线地调用 `cargo metadata` 重新解析 crates.io。国内直连易卡死，请先在**同一个终端**
# 导出一个能覆盖 index.crates.io / static.crates.io 的通用代理：
#   $env:HTTP_PROXY='http://<your-proxy>:<port>'; $env:HTTPS_PROXY=$env:HTTP_PROXY
# 依赖已全部本地缓存后，日常可用 `... check_translate.ps1 check --offline`。
[CmdletBinding()]
param(
    [ValidateSet('check', 'build', 'run', 'clippy', 'test', 'bundle', 'pin', 'env')]
    [string]$Mode = 'check',
    # GPU 后端 feature：Windows/Linux 走 Vulkan，NVIDIA 可选 CUDA。二者都隐含基础 translate。
    [ValidateSet('translate', 'translate-vulkan', 'translate-cuda')]
    [string]$Feature = 'translate-vulkan',
    [switch]$Release,
    # CMake 生成器：留空=自动（按本机 CMake 版本挑认识的 VS 生成器，都不认识则用 Ninja）；
    # 也可显式指定，例如 -CMakeGenerator 'Visual Studio 17 2022' 或 -CMakeGenerator Ninja。
    [string]$CMakeGenerator = '',
    # 构建输出 base 目录：仓库路径偏深时（llama Vulkan 子项目超 CMake 250 字符安全上限），
    # 默认自动把 cargo/dx 的构建目录搬到短 base（缺省 C:\dst，dx 与 cargo 都读
    # CARGO_TARGET_DIR）。-TargetDir 显式指定 base；-KeepTargetDir 关除此行为、保持
    # 在项目内 target\（路径偏深时可能编译失败）。
    [string]$TargetDir = '',
    [switch]$KeepTargetDir,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Rest
)

$ErrorActionPreference = 'Stop'

# 仓库根 = 本脚本所在 scripts/ 的上一级。
$Repo = Split-Path -Parent $PSScriptRoot
Set-Location $Repo

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Warn($msg) { Write-Host "!!! $msg" -ForegroundColor Yellow }

# VS 主版本 -> 该 VS 对应生成器所需的最低 CMake 版本，以及生成器名字。
# 关键："Visual Studio 18 2026" 生成器是 CMake 4.2 才加入的；老 CMake 不认。
function Get-VsGeneratorInfo {
    param([int]$Major)
    switch ($Major) {
        18 { return @{ Min = @(4, 2); Name = 'Visual Studio 18 2026' } }
        17 { return @{ Min = @(3, 21); Name = 'Visual Studio 17 2022' } }
        16 { return @{ Min = @(3, 14); Name = 'Visual Studio 16 2019' } }
        15 { return @{ Min = @(3, 0); Name = 'Visual Studio 15 2017' } }
        14 { return @{ Min = @(3, 0); Name = 'Visual Studio 14 2015' } }
        default { return @{ Min = @(3, 0); Name = $null } }
    }
}

function Get-CMakeVersion {
    $line = (& cmake --version 2>$null | Select-Object -First 1)
    if ($line -match '(\d+)\.(\d+)') { return @{ Major = [int]$matches[1]; Minor = [int]$matches[2] } }
    return $null
}

function Test-CMakeSupportsGenerator {
    param($CMakeVer, [int]$VsMajor)
    if (-not $CMakeVer) { return $true }   # 拿不到 cmake 版本就别乱改，交给默认逻辑
    $min = (Get-VsGeneratorInfo $VsMajor).Min
    if ($CMakeVer.Major -gt $min[0]) { return $true }
    if ($CMakeVer.Major -eq $min[0] -and $CMakeVer.Minor -ge $min[1]) { return $true }
    return $false
}

# ---------------------------------------------------------------------------
# 1) MSVC 开发者环境 + CMake 生成器对齐：
#    a. 把 cl/link/nmake 及 Windows SDK 头/库导入当前会话（已在 Developer
#       PowerShell 里则跳过）；
#    b. 处理「cmake crate 探测到的最新 VS ↔ 已安装 CMake 版本」的生成器错位，
#       显式设置 CMAKE_GENERATOR（cmake crate 会读这个环境变量覆盖自动探测，
#       见其 lib.rs：generator.or_else(|| getenv("CMAKE_GENERATOR"))）。
# ---------------------------------------------------------------------------
function Import-VsDevEnv {
    $cmVer = Get-CMakeVersion

    # 用 vswhere 枚举装了 MSVC x64 生成工具的 VS，按主版本从高到低。
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $candidates = @()
    if (Test-Path $vswhere) {
        # 不用 -format json：JSON 里的 displayName/description 含中文，在重定向或
        # 非 UTF-8 代码页下会被解码成乱码甚至破坏 JSON 结构，导致 ConvertFrom-Json 报
        # 「传入的对象无效」。改用 value 格式只取两个 ASCII 字段；本机器 vswhere 不支持
        # 逗号多属性，故分两次查询同一筛选条件（输出顺序一致）后按序号配对。
        $vsArgs = @('-products', '*', '-prerelease', '-requires',
                    'Microsoft.VisualStudio.Component.VC.Tools.x86.x64')
        $vers = @(& $vswhere @vsArgs -property installationVersion 2>$null | ForEach-Object { "$_".Trim() } | Where-Object { $_ })
        $paths = @(& $vswhere @vsArgs -property installationPath 2>$null | ForEach-Object { "$_".Trim() } | Where-Object { $_ -and (Test-Path $_) })
        for ($i = 0; $i -lt [Math]::Min($vers.Count, $paths.Count); $i++) {
            $major = 0
            if ($vers[$i] -match '^(\d+)') { $major = [int]$Matches[1] } else { continue }
            $candidates += [pscustomobject]@{ Path = $paths[$i]; Major = $major }
        }
    }
    $candidates = @($candidates | Sort-Object Major -Descending)

    # 选「本机 CMake 认识其生成器」的最高 VS；都不认识则取最高版本 VS 配 Ninja。
    $chosen = $null
    $genName = $null
    foreach ($c in $candidates) {
        if (Test-CMakeSupportsGenerator -CMakeVer $cmVer -VsMajor $c.Major) {
            $chosen = $c
            $genName = (Get-VsGeneratorInfo $c.Major).Name
            break
        }
    }
    if (-not $chosen -and $candidates.Count -gt 0) {
        $chosen = $candidates[0]
        if (Get-Command ninja -ErrorAction SilentlyContinue) {
            $genName = 'Ninja'
        }
        else {
            throw (("已装的 VS(v{0})的『Visual Studio 20xx』生成器需要 CMake >= 4.2，而本机 CMake 是 {1}.{2}。" -f `
                $chosen.Major, $cmVer.Major, $cmVer.Minor) +
                " 请任选其一：升级 CMake 到 >= 4.2 / 安装 VS 2022 Build Tools / 装 ninja.exe 后用 -CMakeGenerator Ninja。")
        }
    }

    if (Get-Command cl.exe -ErrorAction SilentlyContinue) {
        Write-Step "检测到 cl.exe 已在 PATH（疑似 Developer PowerShell），跳过 vcvars 导入"
    }
    elseif (-not $chosen) {
        throw "未找到带 MSVC x64 生成工具的 Visual Studio。请安装「使用 C++ 的桌面开发」工作负载，或从『Developer PowerShell for VS』运行本脚本。"
    }
    else {
        $devcmd = Join-Path $chosen.Path 'Common7\Tools\VsDevCmd.bat'
        if (-not (Test-Path $devcmd)) { throw "未找到 VsDevCmd.bat：$devcmd" }
        Write-Step "导入 VS 开发者环境：$($chosen.Path) (amd64)"
        # 运行 VsDevCmd 后 `set` 打印全部环境变量，回填到当前 PowerShell 会话。
        # VsDevCmd 需要 cmd 扩展。只传 -arch=amd64（与 Rust 的 x86_64-pc-windows-msvc
        # 主机目标匹配；win-arm64 改成 amd64_arm64）与 -no_logo。VS 18/BuildTools 对
        # -arch_host / -nologo 会报“Invalid argument”并非零退出，会把后面的 `&& set`
        # 短路掉导致环境不回填，因此这里不传这些参数。
        $envText = & cmd.exe /c "`"$devcmd`" -arch=amd64 -no_logo && set"
        foreach ($line in $envText) {
            if ($line -match '^([^=]+)=(.*)$') { Set-Item -Path ("env:\" + $matches[1]) -Value $matches[2] }
        }
        if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
            throw "导入后仍找不到 cl.exe，vcvars 可能未成功执行。"
        }
    }

    # 生成器最终选择（优先级：-CMakeGenerator 参数 > 预设 $env:CMAKE_GENERATOR > 自动）。
    # 自动优先 Ninja：本项目的 llama.cpp Vulkan 后端把 vulkan-shaders-gen 作为 CMake
    # ExternalProject_Add 子项目单独 configure；在「VS 多配置生成器 + 从 Rust build
    # script 里跑」的组合下，子项目会出现 "The C compiler identification is unknown /
    # No CMAKE_C_COMPILER could be found"（llama.cpp 在 Rust 里用 VS 生成器编 Vulkan
    # 的已知坑）。Ninja 单配置 + PATH 上的 cl.exe 不受影响，且顺带避开 --parallel
    # 下 MSBuild 高并发的偶发失败。没有 ninja 时才退回本机 CMake 认识的 VS 生成器。
    if ($CMakeGenerator) {
        $env:CMAKE_GENERATOR = $CMakeGenerator
    }
    elseif ($env:CMAKE_GENERATOR) {
        # 用户已在环境里显式设过，尊重之
    }
    elseif (Get-Command ninja -ErrorAction SilentlyContinue) {
        $env:CMAKE_GENERATOR = 'Ninja'
    }
    elseif ($genName) {
        $env:CMAKE_GENERATOR = $genName
    }
    else {
        throw "既没有可用的 ninja，也没有本机 CMake 认识的 Visual Studio 生成器。请装 ninja（推荐），或升级 CMake>=4.2 / 安装受支持的 VS。"
    }
    Write-Step "CMake 生成器锁定：$env:CMAKE_GENERATOR"

    # MSVC 并发编译的 PDB 坑：Ninja --parallel 下多个 cl.exe 同时写同一个 vc140.pdb 会报
    # fatal error C1041，需要 /FS 强制同步写 PDB。父项目的 CMAKE_C_FLAGS 由 build.rs 显式
    # 传入（已含 /FS），但 llama.cpp 的 vulkan-shaders-gen 是 ExternalProject 子项目，
    # configure 时不继承父级 flags——它的缓存只从同名环境变量初始化，所以必须把 /FS 放到
    # env 里才能传导给子项目。采用追加而非覆盖，保留用户已设的其它 flags。
    # 注意：仅对「尚未 configure」的构建目录生效；切生成器或改 flags 后需删掉旧 build 目录。
    if (-not ($env:CMAKE_C_FLAGS -match '(^|\s)/FS(\s|$)')) {
        $env:CMAKE_C_FLAGS = ("$env:CMAKE_C_FLAGS /FS").Trim()
    }
    if (-not ($env:CMAKE_CXX_FLAGS -match '(^|\s)/FS(\s|$)')) {
        $env:CMAKE_CXX_FLAGS = ("$env:CMAKE_CXX_FLAGS /FS").Trim()
    }
    Write-Step '已向 CMAKE_C/CXX_FLAGS 追加 /FS（防并发 cl.exe 写同一 PDB 报 C1041）'

    # 路径长度预检与自动「搬家」：llama.cpp 的 Vulkan 子项目（vulkan-shaders-gen）在 CMake
    # ExternalProject 里单独 configure，其 try-compile 目标文件目录相对仓库根约 241 字符
    # （实测 25 字符的仓库根 => object 目录 266 字符）。超过 CMake 的 250 上限后
    # cl/ninja 可能直接打不开文件（Win32 传统 260 限制），表现为“loading
    # 'CMakeFiles\rules.ninja': The system cannot find the file specified”或 MSB8066。
    # 对策：dx（request.rs：CARGO_TARGET_DIR > cargo config > <repo>\target）和 cargo 都优先读
    # CARGO_TARGET_DIR，把它指到短 base 即可整体削减约 24 字符深度，仓库不动。
    $objDirLen = $Repo.Length + 241
    if ($env:CARGO_TARGET_DIR) {
        Write-Step "CARGO_TARGET_DIR 已由环境指定：$env:CARGO_TARGET_DIR"
    }
    elseif ($KeepTargetDir) {
        if ($objDirLen -gt 250) {
            Write-Warn ("已按要求保持项目内 target\（预估子项目路径约 $objDirLen 字符 > CMake 安全上限 250），若在 vulkan-shaders-gen / TryCompile / rules.ninja 处失败，去掉 -KeepTargetDir 重跑即可自动搬到短 base。")
        }
    }
    elseif ($TargetDir -or $objDirLen -gt 250) {
        $newBase = if ($TargetDir) { $TargetDir } else { 'C:\dst' }
        try {
            New-Item -ItemType Directory -Force $newBase -ErrorAction Stop | Out-Null
            $env:CARGO_TARGET_DIR = $newBase
            Write-Step ("仓库路径偏深（预估子项目路径约 $objDirLen 字符 > 250），构建目录已搬到短 base：$newBase")
            Write-Step "首次搬家 cargo 会全量重编一次；安装包产物位置随之变为 $newBase\dx\synstudy\bundle\windows\"
        } catch {
            $env:CARGO_TARGET_DIR = $null
            Write-Warn ("无法创建短 base 目录 $newBase（盘不存在/无权限？），保持项目内 target\。若在子项目处失败，可手动建目录联接：cmd /c mklink /J C:\syn `"$Repo`" 后在 C:\syn 下重跑。")
        }
    }
}

# ---------------------------------------------------------------------------
# 2) LLVM/libclang：bindgen（mnn-sys 等）需要 clang 驱动与 libclang 运行时「同源」。
#    AMD ROCm 会把自家 clang 装到 PATH 最前；若它先被探测到，其系统头目录
#   （ROCm\lib\clang\<ver>\include）会被喂给另一个版本的 libclang，builtin 集合对不上，
#    解析 MNN Interpreter.hpp 时报一串 use of undeclared identifier '__builtin_ia32_*'。
#    对策：找一套完整的 LLVM（bin 下同时有 libclang.dll 与 clang.exe），把它的 bin
#    前置到 PATH 并设 LIBCLANG_PATH 同目录，使驱动/运行时/头文件三者同源。
# ---------------------------------------------------------------------------
function Initialize-LlvmToolchain {
    $cands = @()
    # 优先用户已有的 LIBCLANG_PATH；再常见独立 LLVM 安装位；再 VS 可选组件「C++ Clang 工具」。
    if ($env:LIBCLANG_PATH -and (Test-Path (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) {
        $cands += $env:LIBCLANG_PATH
    }
    foreach ($p in 'C:\tools\LLVM\bin', 'C:\Program Files\LLVM\bin') {
        if (Test-Path (Join-Path $p 'libclang.dll')) { $cands += $p }
    }
    foreach ($vs in @('C:\Program Files\Microsoft Visual Studio\2022\Community',
                      'C:\Program Files\Microsoft Visual Studio\2022\Professional',
                      'C:\Program Files\Microsoft Visual Studio\2022\Enterprise',
                      'C:\Program Files\Microsoft Visual Studio\18\Community')) {
        foreach ($sub in 'VC\Tools\Llvm\x64\bin', 'VC\Tools\Llvm\bin') {
            $p = Join-Path $vs $sub
            if (Test-Path (Join-Path $p 'libclang.dll')) { $cands += $p }
        }
    }

    $best = $cands | Where-Object { Test-Path (Join-Path $_ 'clang.exe') } | Select-Object -First 1
    if ($best) {
        $env:LIBCLANG_PATH = $best
        $list = $env:PATH -split ';'
        if ($list -notcontains $best) { $env:PATH = "$best;$env:PATH" }
        Write-Step "LLVM/libclang 同源对齐：$best（已前置 PATH 并设 LIBCLANG_PATH，避开 ROCm clang 头文件串扰）"
    }
    else {
        $clang = Get-Command clang.exe -ErrorAction SilentlyContinue
        if ($clang -and $clang.Source -match 'ROCm') {
            Write-Warn "PATH 上只有 ROCm 的 clang（$($clang.Source)）。若 mnn-sys 报 bindgen 生成失败 / __builtin_ia32_* 未声明，请装一套完整 LLVM（winget install LLVM.LLVM）或勾选 VS Installer 的「C++ Clang tools for Windows」后重跑。"
        }
    }
}

# ---------------------------------------------------------------------------
# 3) Vulkan SDK：定位安装目录并设 VULKAN_SDK，把 SDK\Bin（含 glslc）加进 PATH。
#    ggml-vulkan 的 build.rs 依赖这两件事，否则编译期生成 SPIR-V / 链接阶段失败。
# ---------------------------------------------------------------------------
function Initialize-VulkanSdk {
    if ($env:VULKAN_SDK -and (Test-Path (Join-Path $env:VULKAN_SDK 'Lib'))) {
        Write-Step "VULKAN_SDK 已设置：$env:VULKAN_SDK"
    } else {
        # 常见安装位置：C:\VulkanSDK\<version>。取版本号最大的那个。
        $candidates = @()
        $root = 'C:\VulkanSDK'
        if (Test-Path $root) {
            $candidates += Get-ChildItem $root -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name -Descending
        }
        # 注册表兜底（Khronos Vulkan SDK 安装器写 HKLM\...\Vulkan SDK）。
        $reg = 'HKLM:\SOFTWARE\Khronos\Vulkan\SDK'
        if (Test-Path $reg) {
            $installDir = (Get-ItemProperty -Path $reg -ErrorAction SilentlyContinue).InstallDir
            if ($installDir -and (Test-Path $installDir)) {
                $candidates += Get-Item $installDir
            }
        }

        $found = $candidates | Where-Object { Test-Path (Join-Path $_.FullName 'Lib') } | Select-Object -First 1
        if (-not $found) {
            throw "未找到 Vulkan SDK。请从 https://vulkan.lunarg.com/sdk/home#windows 安装，安装时勾选「Add to PATH (VULKAN_SDK)」；或手动设置 `$env:VULKAN_SDK 指向 SDK 根目录后重试。"
        }
        $env:VULKAN_SDK = $found.FullName
        Write-Step "自动定位 Vulkan SDK：$($env:VULKAN_SDK)"
    }

    $bin = Join-Path $env:VULKAN_SDK 'Bin'
    if (($env:PATH -split ';') -notcontains $bin) {
        $env:PATH = "$bin;$env:PATH"
    }
    if (-not (Get-Command glslc -ErrorAction SilentlyContinue)) {
        Write-Warn "PATH 上找不到 glslc（应在 $bin）。Vulkan 后端编译着色器需要它；若构建报 glslc 相关错误，请确认 Vulkan SDK 完整安装。"
    }
}

# ---------------------------------------------------------------------------
# 装配环境（除 pin 外都先配好工具链；env 模式配完即返回，用于排查）
# ---------------------------------------------------------------------------
if ($Mode -ne 'pin') {
    Import-VsDevEnv
    Initialize-LlvmToolchain
    if ($Feature -eq 'translate-vulkan') {
        Initialize-VulkanSdk
    }
}

# bindgen / cbindgen 走系统 git 拉取 git 依赖（rust-paddle-ocr、mnn），比 cargo 内置
# libgit2 更稳（部分企业网络 / 代理下 libgit2 会卡）。
$env:CARGO_NET_GIT_FETCH_WITH_CLI = 'true'

# pin 模式：把传递依赖 mnn/mnn-sys 固定到 rust-paddle-ocr v1.4.2 兼容的 rev。
# 本仓库不跟踪 Cargo.lock，首次 clean 构建 / cargo update 后 mnn 会漂到
# 带 `Session<'i>` 生命周期的 HEAD，编译报 E0106/E0308。跑一次本模式复原。
if ($Mode -eq 'pin') {
    Write-Step "固定 mnn 到 1a275e70（v1.4.2 兼容 rev）"
    & cargo update -p mnn --precise 1a275e70336d2f21603a41b22b3b257bc3b49333
    exit $LASTEXITCODE
}

Write-Step "工具链自检"
foreach ($t in 'cl.exe', 'cmake.exe', 'glslc', 'ninja', 'clang.exe') {
    $c = Get-Command $t -ErrorAction SilentlyContinue
    if ($c) { Write-Host ("  {0,-12} {1}" -f $t, $c.Source) } else { Write-Host ("  {0,-12} <未找到>" -f $t) }
}
Write-Host ("  LIBCLANG_PATH {0}" -f ($(if ($env:LIBCLANG_PATH) { $env:LIBCLANG_PATH } else { '<未设置>' })))
Write-Host ("  VULKAN_SDK    {0}" -f ($(if ($env:VULKAN_SDK) { $env:VULKAN_SDK } else { '<未设置>' })))
Write-Host ("  CMAKE_GENERATOR {0}" -f ($(if ($env:CMAKE_GENERATOR) { $env:CMAKE_GENERATOR } else { '<auto>' })))
Write-Host ("  CARGO_TARGET_DIR {0}" -f ($(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$Repo\target (默认)" })))

if ($Mode -eq 'env') {
    Write-Step "环境已配置（仅演示，未执行构建）。可运行 cargo / dx 了。"
    return
}

# ---------------------------------------------------------------------------
# 4) 组装并执行命令
# ---------------------------------------------------------------------------
$releaseFlag = if ($Release) { @('--release') } else { @() }

switch ($Mode) {
    'check'  { $argv = @('check',  '--features', $Feature) + $releaseFlag + $Rest }
    'build'  { $argv = @('build',  '--features', $Feature) + $releaseFlag + $Rest }
    'run'    { $argv = @('run',    '--features', $Feature) + $releaseFlag + $Rest }
    'clippy' { $argv = @('clippy', '--features', $Feature) + $releaseFlag + $Rest }
    'test'   {
        # 翻译相关单元测试（mt.rs 的提示词模板测试）仅在 feature 开启时编入。
        $argv = @('test', '--features', $Feature) + $Rest
    }
    'bundle' {
        # 产出带 Vulkan 翻译功能的 Windows 安装包（NSIS -setup.exe）。需已安装 dx CLI
        # （cargo install dioxus-cli --version 0.8.0-alpha.1 --locked）。
        if (-not (Get-Command dx -ErrorAction SilentlyContinue)) {
            throw "未找到 dx CLI。请先安装：cargo install dioxus-cli --version 0.8.0-alpha.1 --locked"
        }
        $argv = @('bundle', '--features', $Feature) + $releaseFlag + $Rest
    }
}

if ($Mode -eq 'bundle') {
    Write-Step "dx $($argv -join ' ')"
    & dx @argv
} else {
    Write-Step "cargo $($argv -join ' ')"
    & cargo @argv
}
exit $LASTEXITCODE
