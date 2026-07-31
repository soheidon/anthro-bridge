export const DASHBOARD_GRID_COLUMNS = 3;
// Must match .provider-tile min-height and .provider-tile-grid gap in App.css.
export const DASHBOARD_TILE_MIN_HEIGHT = 94;
export const DASHBOARD_GRID_ROW_GAP = 8;

export type InitialWindowHeightOptions = {
  baseHeight: number;
  baseRows: number;
  rowHeight: number;
  minHeight: number;
  maxHeight?: number;
};

export type AvailableHeightInput = {
  scaleFactor: number;
  outerY: number;
  outerHeight: number;
  innerHeight: number;
  workAreaY: number;
  workAreaHeight: number;
};

function finiteOr(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

export function calculateDashboardGridRows(
  cardCount: number,
  columns = DASHBOARD_GRID_COLUMNS,
): number {
  const safeCount = Number.isFinite(cardCount) && cardCount > 0
    ? Math.floor(cardCount)
    : 0;
  const safeColumns = Number.isFinite(columns) && columns > 0
    ? Math.floor(columns)
    : DASHBOARD_GRID_COLUMNS;

  return safeCount === 0 ? 0 : Math.ceil(safeCount / safeColumns);
}

export function calculateInitialWindowHeight(
  rowCount: number,
  options: InitialWindowHeightOptions,
): number {
  const safeRows = Number.isFinite(rowCount) && rowCount > 0
    ? Math.floor(rowCount)
    : 0;
  const safeBase = Math.max(0, finiteOr(options.baseHeight, 0));
  const safeBaseRows = Math.max(0, finiteOr(options.baseRows, 0));
  const safeRowHeight = Math.max(0, finiteOr(options.rowHeight, 0));
  const safeMin = Math.max(0, finiteOr(options.minHeight, 0));
  const safeMax = options.maxHeight === undefined
    ? undefined
    : Math.max(0, finiteOr(options.maxHeight, 0));

  if (safeMax !== undefined && safeMax < safeMin) {
    return Math.round(safeMin);
  }

  const calculated = safeBase + Math.max(0, safeRows - safeBaseRows) * safeRowHeight;
  const lowerBounded = Math.max(calculated, safeMin);
  const result = safeMax === undefined ? lowerBounded : Math.min(lowerBounded, safeMax);

  return Math.round(result);
}

export function calculateAvailableLogicalHeight(
  input: AvailableHeightInput,
): number | undefined {
  const values = [
    input.scaleFactor,
    input.outerY,
    input.outerHeight,
    input.innerHeight,
    input.workAreaY,
    input.workAreaHeight,
  ];

  if (values.some((value) => !Number.isFinite(value)) || input.scaleFactor <= 0) {
    return undefined;
  }

  const outerHeight = Math.max(0, input.outerHeight);
  const innerHeight = Math.max(0, input.innerHeight);
  const decorationHeight = Math.max(0, outerHeight - innerHeight);
  const workAreaHeight = Math.max(0, input.workAreaHeight);
  const workAreaBottom = input.workAreaY + workAreaHeight;
  const effectiveOuterY = Math.max(input.outerY, input.workAreaY);
  const availableOuterHeight = Math.max(0, workAreaBottom - effectiveOuterY);
  const availableInnerPhysicalHeight = Math.max(
    0,
    availableOuterHeight - decorationHeight,
  );

  return Math.round(availableInnerPhysicalHeight / input.scaleFactor);
}
