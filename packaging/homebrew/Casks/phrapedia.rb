cask "phrapedia" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
  sha256 arm:   "0000000000000000000000000000000000000000000000000000000000000000",
         intel: "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/luangpho/phrapedia/releases/download/v#{version}/Phrapedia_#{version}_#{arch}.dmg"
  name "Phrapedia"
  desc "Local-first knowledge base for Thai monks, temples and amulets"
  homepage "https://github.com/luangpho/phrapedia"

  auto_updates true

  depends_on formula: "node"

  app "Phrapedia.app"

  preflight_steps do
    # ad-hoc 署名アプリのため quarantine 属性のみ除去する（再署名は hardened runtime のフラグを壊すため行わない）
    run "xattr", args: ["-dr", "com.apple.quarantine", "{{staged_path}}/Phrapedia.app"]
  end

  zap trash: [
    "~/Library/Application Support/jp.grrow.phrapedia",
    "~/Library/Caches/jp.grrow.phrapedia",
    "~/Library/Saved Application State/jp.grrow.phrapedia.savedState",
    "~/Library/WebKit/jp.grrow.phrapedia",
  ]
end
