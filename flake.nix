{
  description = "idk";

  inputs = {
    esp-dev.url = "github:mirrexagon/nixpkgs-esp-dev";
  };

  outputs = { self, nixpkgs, esp-dev }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      llvm-xtensa =
        let src = fetchTarball {
              url = "https://github.com/espressif/llvm-project/archive/refs/tags/esp-14.0.0-20220415.tar.gz";
              sha256 = "sha256:1kw9qm2gp4nb30725yqdcnscqzk3z5gks36wnlb3vn4q7iff51c2";
            };
        in pkgs.clangStdenv.mkDerivation {
          name = "llvm-xtensa";
          version = "esp-14.0.0-20220415";

          inherit src;

          buildInputs = [
            pkgs.python3
            pkgs.cmake
            pkgs.ninja
          ];

          phases = [ "unpackPhase" "buildPhase" "installPhase" "fixupPhase" ];

          buildPhase = ''
            mkdir llvm_build
            cd llvm_build
            cmake ../llvm -DLLVM_ENABLE_PROJECTS="clang;libc;libclc;libcxx;libcxxabi;libunwind;lld" -DLLVM_INSTALL_UTILS=ON -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD="Xtensa" -DCMAKE_BUILD_TYPE=Release -G "Ninja"
            cmake --build .
          '';

          installPhase = ''
            mkdir -p $out
            cmake -DCMAKE_INSTALL_PREFIX=$out -P cmake_install.cmake
          '';

          meta = with pkgs.lib; {
            description = "LLVM xtensa";
            license = licenses.asl20;
          };
        };
    in
    {
      packages.x86_64-linux = {
        llvm = llvm-xtensa;
      };
    };
}
