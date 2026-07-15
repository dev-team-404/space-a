fn main() {
    tauri_build::build();
    // tauri-build는 매니페스트 리소스(libresource.a)를 rustc-link-arg-bins로만 링크한다(bin 전용).
    // lib 단위테스트 하니스에 muda의 TaskDialogIndirect(comctl32 v6 전용) 임포트가 딸려 들어오면
    // 매니페스트 부재 → comctl32 5.82 로드 → STATUS_ENTRYPOINT_NOT_FOUND로 로드조차 실패한다
    // (Windows+GNU에서 cargo test -p agent-mentor-app 크래시 실측). rustc-link-arg-tests는 lib
    // 단위테스트에 적용되지 않으므로(cargo #10937) 전 타깃 link-arg로 리소스를 링크한다.
    // (rlib에는 link-arg가 무시되고, bin은 중복 전달돼도 GNU ld가 동일 아카이브를 무해하게 처리.)
    println!(
        "cargo::rustc-link-arg={}/libresource.a",
        std::env::var("OUT_DIR").expect("OUT_DIR")
    );
}
