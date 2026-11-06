/// 请求代次闸门：每次发起异步请求时领取一个编号；清除/关闭/发起新请求都会让
/// 旧编号失效。这样在途请求即使晚返回，也不会把用户已经清掉的内容重新写回界面。
export class RequestGate {
  private generation = 0;

  next(): number {
    this.generation += 1;
    return this.generation;
  }

  invalidate(): void {
    this.generation += 1;
  }

  isCurrent(id: number): boolean {
    return id === this.generation;
  }
}
