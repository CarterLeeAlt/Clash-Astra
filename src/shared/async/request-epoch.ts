/** 请求序号同时约束重新绑定、卸载和较新的流快照，阻止旧结果覆盖当前状态。 */
export class RequestEpoch {
  private revision = 0;
  next() {
    return ++this.revision;
  }
  invalidate() {
    this.revision += 1;
  }
  accepts(ticket: number) {
    return ticket === this.revision;
  }
}
