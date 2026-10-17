class Codifix < Formula
  desc "Local project memory service and terminal UI for coding agents"
  homepage "https://github.com/3vilM33pl3/codifix"
  head "https://github.com/3vilM33pl3/codifix.git", branch: "main"

  depends_on "node" => :build
  depends_on "rust" => :build

  def install
    system "npm", "--prefix", "web", "ci"
    system "npm", "--prefix", "web", "run", "build"
    system "cargo", "build", "--release", "--locked", "--features", "full", "--manifest-path", "Cargo.toml",
           "--package", "codifix-cli", "--bin", "codifix"

    bin.install "target/release/codifix"
    (bash_completion/"codifix").write Utils.safe_popen_read(bin/"codifix", "completion", "bash")
    (zsh_completion/"_codifix").write Utils.safe_popen_read(bin/"codifix", "completion", "zsh")
    (fish_completion/"codifix.fish").write Utils.safe_popen_read(bin/"codifix", "completion", "fish")
    (pkgshare/"skill-template").install Dir[".agents/skills/*"]
    pkgshare.install "codifix.toml.example"
    pkgshare.install "web/dist" => "web"
  end

  def post_install
    system bin/"codifix", "service", "restart-all", "--mark-tui-restart", "--json"
  rescue
    opoo "Codifix installed, but automatic service restart failed. Run `codifix service restart-all`."
  end

  def caveats
    <<~EOS
      Command:
        codifix

      Shared config:
        ~/Library/Application Support/codifix/codifix.toml

      Shared env:
        ~/Library/Application Support/codifix/codifix.env

      First run:
        codifix wizard --global
        codifix service enable

      To test unreleased changes instead:
        brew reinstall --HEAD 3vilM33pl3/codifix/codifix

      `codifix service enable` provisions the shared service API token
      automatically if it is missing or still set to the development placeholder.

      Optional watcher:
        codifix watcher enable --project <slug>
    EOS
  end

  test do
    assert_match "codifix", shell_output("#{bin}/codifix --help")
    assert_predicate pkgshare/"skill-template", :directory?
    assert_predicate pkgshare/"skill-template/codifix/SKILL.md", :exist?
    assert_predicate pkgshare/"skill-template/codifix-query-resume/SKILL.md", :exist?
    assert_predicate pkgshare/"web/index.html", :exist?
    assert_predicate bash_completion/"codifix", :exist?
    assert_predicate zsh_completion/"_codifix", :exist?
    assert_predicate fish_completion/"codifix.fish", :exist?
  end
end
