import { getAxios } from "./client";

/// 读取内核版本。
export const getVersion = async () => {
  const instance = await getAxios();
  return instance.get("/version") as Promise<{
    version: string;
  }>;
};

/// 读取内核当前实际配置。
export const getClashConfig = async () => {
  const instance = await getAxios();
  return instance.get("/configs") as Promise<IConfigData>;
};

/// 请求内核更新地理规则数据。
export const updateGeoData = async () => {
  const instance = await getAxios();
  return instance.post("/configs/geo");
};

/// 读取实际规则列表。
export const getRules = async () => {
  const instance = await getAxios();
  const response = await instance.get<any, any>("/rules");
  return response?.rules as IRuleItem[];
};

// 读取内核原始代理表。
const getProxiesInner = async () => {
  const instance = await getAxios();
  const response = await instance.get<any, any>("/proxies");
  return (response?.proxies || {}) as Record<string, IProxyItem>;
};

/// 合并代理表与提供方数据，构造页面需要的分组视图。
export const getProxies = async () => {
  const [proxyRecord, providerRecord] = await Promise.all([
    getProxiesInner(),
    getProxyProviders(),
  ]);
  // 建立提供方节点索引。
  const providerMap = Object.fromEntries(
    Object.entries(providerRecord).flatMap(([provider, item]) =>
      item.proxies.map((p) => [p.name, { ...p, provider }])
    )
  );

  // 分组引用可能来自内核代理表，也可能来自代理提供方。
  const generateItem = (name: string) => {
    if (proxyRecord[name]) return proxyRecord[name];
    if (providerMap[name]) return providerMap[name];
    return {
      name,
      type: "unknown",
      udp: false,
      xudp: false,
      tfo: false,
      history: [],
    };
  };

  const { GLOBAL: global, DIRECT: direct, REJECT: reject } = proxyRecord;

  let groups = Object.values(proxyRecord)
    .filter((each) => each.name !== "GLOBAL" && each.all)
    .map((each) => ({
      ...each,
      all: each.all!.map((item) => generateItem(item)),
    }));

  if (global?.all) {
    let globalGroups = global.all
      .filter((name) => proxyRecord[name]?.all)
      .map((name) => proxyRecord[name])
      .map((each) => ({
        ...each,
        all: each.all!.map((item) => generateItem(item)),
      }));
    let globalNames = globalGroups.map((each) => each.name);
    groups = groups
      .filter((group) => {
        return !globalNames.includes(group.name);
      })
      .concat(globalGroups);
  }

  const proxies = [direct, reject].concat(
    Object.values(proxyRecord).filter(
      (p) => !p.all?.length && p.name !== "DIRECT" && p.name !== "REJECT"
    )
  );

  const _global: IProxyGroupItem = {
    ...global,
    all: global?.all?.map((item) => generateItem(item)) || [],
  };

  return { global: _global, direct, groups, records: proxyRecord, proxies };
};

// 读取可由用户更新的代理提供方。
export const getProxyProviders = async () => {
  const instance = await getAxios();
  const response = await instance.get<any, any>("/providers/proxies");

  const providers = (response.providers || {}) as Record<
    string,
    IProxyProviderItem
  >;

  return Object.fromEntries(
    Object.entries(providers).filter(([, item]) => {
      const type = item.vehicleType.toLowerCase();
      return type === "http" || type === "file";
    })
  );
};

export const getRuleProviders = async () => {
  const instance = await getAxios();
  const response = await instance.get<any, any>("/providers/rules");

  const providers = (response.providers || {}) as Record<
    string,
    IRuleProviderItem
  >;

  return Object.fromEntries(
    Object.entries(providers).filter(([, item]) => {
      const type = item.vehicleType.toLowerCase();
      return type === "http" || type === "file";
    })
  );
};

// 触发提供方健康检查。
export const providerHealthCheck = async (name: string) => {
  const instance = await getAxios();
  return instance.get(
    `/providers/proxies/${encodeURIComponent(name)}/healthcheck`
  );
};

export const proxyProviderUpdate = async (name: string) => {
  const instance = await getAxios();
  return instance.put(`/providers/proxies/${encodeURIComponent(name)}`);
};

export const ruleProviderUpdate = async (name: string) => {
  const instance = await getAxios();
  return instance.put(`/providers/rules/${encodeURIComponent(name)}`);
};

export const getConnections = async () => {
  const instance = await getAxios();
  const result = await instance.get("/connections");
  return result as any as IConnections;
};

// 关闭指定连接。
export const deleteConnection = async (id: string) => {
  const instance = await getAxios();
  await instance.delete<any, any>(`/connections/${encodeURIComponent(id)}`);
};

// 关闭全部连接。
export const closeAllConnections = async () => {
  const instance = await getAxios();
  await instance.delete<any, any>(`/connections`);
};
