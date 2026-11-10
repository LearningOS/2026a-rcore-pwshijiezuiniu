# 第五章实验报告

## 一、实验实现

本章实现了 spawn、set_priority 和 stride 调度：spawn 新建地址空间并挂到父进程；set_priority 校验优先级不小于 2，按大常量除以优先级设置 pass；调度器选取 stride 最小的任务运行，调度时累加 pass，使 CPU 时间按优先级分配。另实现取时系统调用，并移植 mmap/munmap，支持重叠检查与部分解除映射。

## 二、问答作业

### 1. 8 位无符号 stride 溢出后，下一次会轮到 p1 吗？

**答：** 因为会溢出，实际轮到 p2 执行。

### 2. 为什么优先级均不小于 2 时，`STRIDE_MAX - STRIDE_MIN <= BigStride / 2`？

**答：** 假设 `STRIDE_MAX` 是上一个调度的进程，那么 `pass >= BIG_STRIDE / 2`；又因为它是上一个调度的，所以 `STRIDE_MAX - pass <= STRIDE_MIN`，可以得出结论。假设不是上一个调度的，那么上个调度的 `a` 只可能 `<= STRIDE_MIN`；这样的话，在 `STRIDE_MAX` 调度上一次的时候一定 `<= STRIDE_MIN`，那么 `pass >= STRIDE_MAX - STRIDE_MIN`，也就是 `BIG_STRIDE / 2 >= STRIDE_MAX - STRIDE_MIN`。

### 3. 带回绕的 Stride 比较器

**答：** 用第二题的结论：如果 `a - b` 的绝对值小于 `u64` 最大值的一半，就按正常顺序比较；否则把比较结果取反。实现如下：

```rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.0.abs_diff(other.0) < u64::MAX / 2 {
            Some(self.0.cmp(&other.0))
        } else {
            Some(other.0.cmp(&self.0))
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}
```

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与以下各位就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

无。

2. 此外，我也参考了以下资料，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

无额外参考资料。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
