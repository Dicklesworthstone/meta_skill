# frozen_string_literal: true

# ms - Meta Skill CLI
# A local-first skill management platform with dual persistence, hybrid search,
# bandit optimization, and native AI agent integration via MCP.
#
# This formula is auto-updated by GitHub Actions when new releases are published.
# Manual edits will be overwritten on the next release.

class Ms < Formula
  desc "Meta Skill - Local-first skill management platform for AI agents"
  homepage "https://github.com/Dicklesworthstone/meta_skill"
  version "0.1.0"
  license "MIT"

  # Platform-specific binaries
  on_macos do
    on_arm do
      url "https://github.com/Dicklesworthstone/meta_skill/releases/download/v#{version}/ms-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_ARM64"

      def install
        bin.install "ms"
        generate_completions_from_executable(bin/"ms", "completions")
      end
    end

    on_intel do
      url "https://github.com/Dicklesworthstone/meta_skill/releases/download/v#{version}/ms-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_X64"

      def install
        bin.install "ms"
        generate_completions_from_executable(bin/"ms", "completions")
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Dicklesworthstone/meta_skill/releases/download/v#{version}/ms-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"

      def install
        bin.install "ms"
        generate_completions_from_executable(bin/"ms", "completions")
      end
    end

    on_intel do
      url "https://github.com/Dicklesworthstone/meta_skill/releases/download/v#{version}/ms-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X64"

      def install
        bin.install "ms"
        generate_completions_from_executable(bin/"ms", "completions")
      end
    end
  end

  def caveats
    <<~EOS
      To get started with ms:

        1. Initialize global configuration:
           ms init --global

        2. Configure skill paths:
           ms config skill_paths.project '["./skills"]'

        3. Index your skills:
           ms index

        4. Search for skills:
           ms search "error handling"

      For more information, see:
        https://github.com/Dicklesworthstone/meta_skill#readme

      Shell completions have been installed. You may need to restart your shell
      or source your shell configuration for them to take effect.
    EOS
  end

  test do
    # Test version output
    assert_match "ms #{version}", shell_output("#{bin}/ms --version").strip

    # Test help command
    help_output = shell_output("#{bin}/ms --help")
    assert_match "skill", help_output.downcase

    # Test doctor command (quick health check)
    # Note: This may have warnings if not initialized, but should not error
    system "#{bin}/ms", "doctor"

    # Test init creates config directory
    system "#{bin}/ms", "init"
    assert_predicate testpath/".ms", :exist?
  end
end
