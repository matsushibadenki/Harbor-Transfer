# Phase 6 — S3 安全設計

## 目的

Phase 6 の最初の成果物は、Amazon S3 と S3 互換ストレージを安全に閲覧し、転送できる接続である。オブジェクトストレージを通常のファイルシステムと同一視せず、フォルダ作成・rename・削除・同期は安全条件を個別に満たしてから段階的に有効化する。

## 接続モデル

ブックマークへ保存する非機密情報は、表示名、リージョン、バケット、任意の HTTPS エンドポイント、初期 prefix、path-style 指定とする。Access Key ID、Secret Access Key、Session Token はひとまとまりの資格情報として macOS Keychain にだけ保存し、SQLite、ブックマーク書き出し、接続履歴、診断ログへ含めない。

AWS SDK の環境変数・共有設定ファイルを探索する既定認証チェーンは使用しない。ブックマークに対応する Keychain 資格情報を明示的な credential provider として渡し、別プロファイルやCI用権限を意図せず利用することを防ぐ。

カスタムエンドポイントは `https://` のみ許可する。TLS証明書検証を無効化する設定は設けない。path-style は MinIO など互換サービス向けの明示設定とし、AWS S3 では既定のvirtual-hosted styleを使う。

## バケットとprefix

S3にはディレクトリが存在しないため、UIの `/photos/2026` はオブジェクトキーprefix `photos/2026/` に変換する。`ListObjectsV2`へprefixとdelimiter `/`を渡し、`CommonPrefixes`をフォルダ、`Contents`をファイルとして表示する。0 byteのフォルダマーカーは重複表示しない。

一覧は継続トークンを使ってページングし、一度の画面取得は最大10,000項目に制限する。上限を超える場合は黙って欠落させず、利用者へprefixを絞るようエラーを返す。

閲覧だけに必要な最小IAM権限は対象バケット／prefixに対する `s3:ListBucket` とする。ダウンロードとcopyには対象objectの`s3:GetObject`、アップロードと確実な取消には`s3:PutObject`と`s3:AbortMultipartUpload`、renameと削除には`s3:DeleteObject`を追加する。初期接続確認も`ListObjectsV2`を使い、不要な`ListAllMyBuckets`権限は要求しない。

## 機能境界

- 一覧、仮想フォルダ移動、検索、リスト／アイコン／カラム表示を有効にする。
- 単一／フォルダアップロード、ダウンロード、Finderへのファイル／フォルダのドラッグ書き出しを有効にする。
- 通常フォルダはobject keyのprefixだけで表現し、空フォルダを0 byte markerで保持するかブックマーク単位で明示設定する。
- アップロードは最小8 MiBのmultipart uploadとし、10,000 part以内に収まるようオブジェクトサイズに応じてpartを拡大する。停止／取消はpart境界で反映し、失敗または取消時は`AbortMultipartUpload`を試みて完了済みpartを放置しない。
- ダウンロードは転送先と同じディレクトリの一時ファイルへストリーミングし、完了後にだけ目的パスへrenameする。失敗または取消時は一時ファイルを削除する。
- renameは全objectの`CopyObject`／multipart copyとサイズ検証が成功してから元objectを削除する。途中失敗時は元データを保持し、destinationに残ったcopy件数をエラーへ含める。
- file／prefix削除はbucket rootを拒否し、1操作100,000 objectの安全上限を設ける。
- 同期ではS3のETagを汎用MD5として扱わず、サイズ・更新日時・利用可能なchecksum metadataを明示的に比較する。

## 公式仕様

- [AWS SDK for Rust credential providers](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html)
- [Configuring client endpoints in the AWS SDK for Rust](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/endpoints.html)
- [Amazon S3 ListObjectsV2](https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html)
- [Organizing objects using prefixes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-prefixes.html)

## 受け入れ条件

- Keychain資格情報を明示して、指定バケットのルートまたは初期prefixへ接続できる。
- 1,000件を超える一覧を継続トークンで取得できる。
- `CommonPrefixes`、Unicodeキー、空のprefixを既存`FileEntry`へ安全に変換できる。
- HTTPエンドポイント、空のリージョン／バケット、不完全な資格情報を接続前に拒否する。
- フォルダアップロード、明示的な空フォルダmarker、rename、削除、同期が既存UIと安全確認へ統合される。
- multipartアップロードとストリーミングダウンロードが既存の進捗、停止、再開、取消イベントへ接続される。
- 失敗したダウンロードが完成ファイルとして残らず、失敗したmultipart uploadに対してabortが呼ばれる。
- MinIOの隔離テストで17 MiB超multipartの往復、Unicode key、取消後の未完了upload消去、1,001件のページング一覧を検証する。
