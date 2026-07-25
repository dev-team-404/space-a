fn main() {
    tauri_build::build();
    // tauri-build는 매니페스트 리소스(libresource.a)를 rustc-link-arg-bins로만 링크한다(bin 전용).
    // lib 단위테스트 하니스에 muda의 TaskDialogIndirect(comctl32 v6 전용) 임포트가 딸려 들어오면
    // 매니페스트 부재 → comctl32 5.82 로드 → STATUS_ENTRYPOINT_NOT_FOUND로 로드조차 실패한다
    // (Windows+GNU에서 cargo test -p agent-mentor-app 크래시 실측). rustc-link-arg-tests는 lib
    // 단위테스트에 적용되지 않으므로(cargo #10937) 전 타깃 link-arg로 리소스를 링크한다.
    // (rlib에는 link-arg가 무시되고, bin은 중복 전달돼도 GNU ld가 동일 아카이브를 무해하게 처리.)
    // 이 전 타깃 link-arg는 GNU 전용 우회다. MSVC에서는 tauri-build의
    // rustc-link-arg-bins(resource.lib)가 이미 bin을 커버하고, 같은 리소스를 한 번 더
    // 링크하면 link.exe가 중복 VERSION 리소스로 실패한다(CVT1100/LNK1123 실측).
    // GNU ld만 동일 아카이브 중복을 무해하게 처리한다.
    // libresource.a는 Windows(GNU)에서만 tauri-build가 생성한다. macOS/Linux에서는
    // 존재하지 않으므로 target_os=="windows" 로도 가드해야 한다. (그러지 않으면
    // non-msvc인 macOS/Linux에서 없는 아카이브를 링크하려다 clang: no such file 로 실패.)
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows"
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        println!(
            "cargo::rustc-link-arg={}/libresource.a",
            std::env::var("OUT_DIR").expect("OUT_DIR")
        );
    }
}
