# get hands dirty：做一个图片服务器有多难？

> 陈天・Rust 编程第一课

## 目标

- 实现一个简单的 `http server`
- 使用 `prost`,`prost-build` 实现对 proto消息的支持
- 使用 `photon-rs` 处理图像
- 使用 `reqwest` 下载远程图像

## 实现

- 原课件中使用了 axum, 本仓库使用了actix-web
- 原课件支持`LRU缓存`, 本仓库上不支持
  - 应当使用类似于memcache or redis来实现

### 简单实现了 `actix-prost` 功能

考虑下面的函数:

```rust
async fn img_proc_request(req_body: Proto<ImageCommand>) -> HttpResult {
  ...
}
```

### 使用宏替代手动match ImageProcessor

参考下面的宏调用: 手动将Enum中的数据填入

*如果使用派生宏实现`运行时反射`,会更加方便.*

```rust
impl_image_proc!(Op, Resize, Rotate);
```

