// 固定四个检查距离，分别覆盖总览、工艺中景、建筑近景和设备近景。
// 该入口只生成 Chrome URL；浏览器启动和截图遵循仓库的 Google Chrome 规则。

const REVIEW_VIEWS = Object.freeze([
  Object.freeze({
    name: 'overview',
    purpose: '厂区总体布局、道路和工艺分区',
    target: Object.freeze([300, 4, 0]),
    orientation: Object.freeze([45, 26]),
    distance: 360,
  }),
  Object.freeze({
    name: 'process-mid',
    purpose: '生化池、二沉池和池顶设备的整体关系',
    target: Object.freeze([300, 4, 35]),
    orientation: Object.freeze([45, 24]),
    distance: 58,
  }),
  Object.freeze({
    name: 'building-close',
    purpose: '建筑立面、门窗、百叶、屋面和落水系统',
    target: Object.freeze([300, 5, 76]),
    orientation: Object.freeze([42, 18]),
    distance: 42,
  }),
  Object.freeze({
    name: 'equipment-close',
    purpose: '橇块、电机、控制箱、电缆桥架和管道附件',
    target: Object.freeze([62, 2.2, 10]),
    orientation: Object.freeze([35, 18]),
    distance: 30,
  }),
]);

export function buildWastewaterVisualReview({
  baseUrl = 'http://127.0.0.1:8150',
  token = 'wastewater-review',
} = {}) {
  const origin = new URL(baseUrl);
  return REVIEW_VIEWS.map((view) => {
    const url = new URL(origin);
    url.searchParams.set('token', token);
    url.searchParams.set('target', view.target.join(','));
    url.searchParams.set(
      'view',
      [...view.orientation, view.distance].join(','),
    );
    return {
      name: view.name,
      purpose: view.purpose,
      target: [...view.target],
      orientation: [...view.orientation],
      distance: view.distance,
      url: url.toString(),
    };
  });
}

if (process.argv[1] && import.meta.url === new URL(
  `file://${process.argv[1]}`,
).href) {
  const views = buildWastewaterVisualReview({
    baseUrl: process.env.STUDIO_URL ?? 'http://127.0.0.1:8150',
    token: process.env.STUDIO_TOKEN ?? 'wastewater-review',
  });
  console.log(JSON.stringify(views, null, 2));
}
