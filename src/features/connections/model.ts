export const initConn: IConnections = {
  uploadTotal: 0,
  downloadTotal: 0,
  connections: [],
};

type ConnectionOrderFunc = (list: IConnectionsItem[]) => IConnectionsItem[];

const getActiveSpeed = (conn: IConnectionsItem) =>
  (conn.curDownload ?? 0) + (conn.curUpload ?? 0);

const getTotalTraffic = (conn: IConnectionsItem) =>
  (conn.download ?? 0) + (conn.upload ?? 0);

const getStartTimestamp = (conn: IConnectionsItem) => {
  if (!conn.start) return 0;
  const ts = new Date(conn.start).getTime();
  return Number.isNaN(ts) ? 0 : ts;
};

/**
 * 用 Map 按 id 匹配新旧快照，保持旧条目原有位置以减少列表跳动（O(n)）。
 */
export const mergeConnectionSnapshot = (
  oldConn: IConnectionsItem[],
  incoming: IConnectionsItem[]
): IConnectionsItem[] => {
  const maxLen = incoming.length;
  const oldIndexMap = new Map(oldConn.map((conn, index) => [conn.id, index]));
  const connections: IConnectionsItem[] = new Array(maxLen);
  const rest: IConnectionsItem[] = [];

  incoming.forEach((each) => {
    const next = { ...each };
    const index = oldIndexMap.get(each.id);
    if (index !== undefined && index < maxLen) {
      const prev = oldConn[index];
      next.curUpload = Math.max(0, (each.upload ?? 0) - (prev.upload ?? 0));
      next.curDownload = Math.max(
        0,
        (each.download ?? 0) - (prev.download ?? 0)
      );
      connections[index] = next;
    } else {
      next.curUpload = 0;
      next.curDownload = 0;
      rest.push(next);
    }
  });

  let restIndex = 0;
  for (let i = 0; i < maxLen && restIndex < rest.length; ++i) {
    if (!connections[i]) {
      connections[i] = rest[restIndex++];
    }
  }
  return connections;
};

const sortByActiveSpeed = (list: IConnectionsItem[]) =>
  list
    .map((item, index) => ({ item, index }))
    .sort((a, b) => {
      const activeDiff = getActiveSpeed(b.item) - getActiveSpeed(a.item);
      if (activeDiff !== 0) return activeDiff;

      const trafficDiff = getTotalTraffic(b.item) - getTotalTraffic(a.item);
      if (trafficDiff !== 0) return trafficDiff;

      const startDiff = getStartTimestamp(b.item) - getStartTimestamp(a.item);
      if (startDiff !== 0) return startDiff;

      return a.index - b.index;
    })
    .map(({ item }) => item);

export const connectionOrderOptions: Record<string, ConnectionOrderFunc> = {
  "Active Speed": sortByActiveSpeed,
  Default: (list) => list,
  "Upload Speed": (list) =>
    [...list].sort((a, b) => (b.curUpload ?? 0) - (a.curUpload ?? 0)),
  "Download Speed": (list) =>
    [...list].sort((a, b) => (b.curDownload ?? 0) - (a.curDownload ?? 0)),
};
