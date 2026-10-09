# Dependency installation router for iNtoo
# This script is meant to be sourced.

# shellcheck shell=bash

printf "${STY_CYAN}[$0]: 1. Install dependencies${STY_RST}\n"

#####################################################################################
# Route to the appropriate installer based on OS
#####################################################################################

case "$OS_GROUP_ID" in
  arch)
    printf "${STY_GREEN}Using Arch Linux installer${STY_RST}\n"
    source ./sdata/dist-arch/install-deps.sh
    ;;
    
  fedora)
    printf "${STY_GREEN}Using Fedora installer${STY_RST}\n"
    source ./sdata/dist-fedora/install-deps.sh
    ;;
    
  debian|ubuntu)
    printf "${STY_GREEN}Using Debian/Ubuntu installer${STY_RST}\n"
    source ./sdata/dist-debian/install-deps.sh
    ;;
    
  opensuse)
    printf "${STY_YELLOW}openSUSE support is experimental${STY_RST}\n"
    printf "${STY_YELLOW}Using generic installer with guidance${STY_RST}\n"
    source ./sdata/dist-generic/install-deps.sh
    ;;
    
  void)
    printf "${STY_YELLOW}Void Linux support is experimental${STY_RST}\n"
    printf "${STY_YELLOW}Using generic installer with guidance${STY_RST}\n"
    source ./sdata/dist-generic/install-deps.sh
    ;;
    
  gentoo)
    printf "${STY_GREEN}Using Gentoo Portage installer${STY_RST}\n"
    source ./sdata/dist-gentoo/install-deps.sh
    ;;
    
  nixos)
    printf "${STY_YELLOW}NixOS requires declarative configuration${STY_RST}\n"
    echo ""
    echo "For NixOS, add iNtoo to your configuration.nix or home-manager."
    echo "See: https://github.com/D0UP1G/iNtoo/tree/main/docs/NixOS"
    echo ""
    echo "Basic steps:"
    echo "  1. Add quickshell and niri to your system packages"
    echo "  2. Clone this repo to ~/.config/quickshell/inir"
    echo "  3. Run: ./setup install --skip-deps"
    echo ""
    
    if $ask; then
      read -p "Continue with --skip-deps? [y/N]: " choice
      if [[ ! "$choice" =~ ^[yY]$ ]]; then
        exit 0
      fi
    fi
    
    # Skip to file installation
    return 0
    ;;
    
  alpine)
    printf "${STY_YELLOW}Alpine Linux support is experimental${STY_RST}\n"
    printf "${STY_YELLOW}Using generic installer with guidance${STY_RST}\n"
    source ./sdata/dist-generic/install-deps.sh
    ;;
    
  generic|*)
    printf "${STY_YELLOW}Using generic installer${STY_RST}\n"
    source ./sdata/dist-generic/install-deps.sh
    ;;
esac

_inir_deps_status=$?
[[ "$_inir_deps_status" -eq 0 ]] || return "$_inir_deps_status"
source ./sdata/lib/install-opencode.sh
install_opencode
