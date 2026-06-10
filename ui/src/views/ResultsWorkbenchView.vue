<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, h, onBeforeUnmount, ref, watch, nextTick } from "vue";
import { useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NCard,
  NRadioGroup,
  NRadioButton,
  NDataTable,
  NEmpty,
  NGi,
  NGrid,
  NInput,
  NModal,
  NSelect,
  NSpace,
  NStatistic,
  NTag,
  NText,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import VChart from "vue-echarts";
import { use } from "echarts/core";
import { PieChart, TreemapChart } from "echarts/charts";
import {
  LegendComponent,
  TitleComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { useAppStore } from "@/stores/app";
import { useAiFile } from "@/composables/useAiFile";
import { useScan } from "@/composables/useScan";
import AppOverviewSection from "@/components/AppOverviewSection.vue";
import type {
  DuplicateGroup,
  AppOverviewRow,
  FileAiInsight,
  FileRecord,
  FileSuggestion,
  FileTreeQueryResult,
  FileTreeRow,
  GovernanceSuggestion,
  ScanModuleKind,
  ScanModuleSummary,
  SuggestedAction,
} from "@/types";
import candlestickLayout from "echarts/types/src/chart/candlestick/candlestickLayout.js";
import { convertCompilerOptionsFromJson } from "typescript";
import { onMounted } from "vue";

type FileCategory =
  | "all"
  | "image"
  | "video"
  | "audio"
  | "archive"
  | "executable"
  | "document"
  | "code"
  | "other";

const TEXT = {
  localRules: "本地规则分析",
  remoteModel: "远程 AI 模型：",
  fileType: "文件类型",
  noExt: "无扩展名",
  filePath: "路径",
  size: "大小",
  ext: "扩展名",
  noScanResult: "还没有扫描结果。",
  goScan: "去扫描",
  overview: "概览",
  rootPath: "扫描根目录",
  totalFiles: "文件总数",
  totalSize: "总大小",
  duplicateGroups: "重复文件组",
  suggestionCount: "建议数",
  typeDistribution: "文件类型分布",
  scanModules: "扫描模块",
  directoryOverview: "目录概览",
  itemName: "名称",
  itemType: "类型",
  fileCount: "文件数",
  contentPreview: "内容预览",
  directory: "目录",
  file: "文件",
  scannedFiles: "分层文件清单",
  scannedFilesHintPrefix: "共匹配 ",
  scannedFilesHintMiddle: " 个文件，当前展示 ",
  scannedFilesHintSuffix: " 个树节点。",
  fileSearchPlaceholder: "按文件名、路径或扩展名筛选，例如 pdf、src、README",
  fileCategoryPlaceholder: "按文件类型筛选",
  categoryAll: "全部文件",
  categoryImage: "图片文件",
  categoryVideo: "视频文件",
  categoryAudio: "音频文件",
  categoryArchive: "压缩包 / 镜像",
  categoryExecutable: "可执行 / 安装 / 脚本",
  categoryDocument: "文档文件",
  categoryCode: "代码 / 配置",
  categoryOther: "其他文件",
  largeFiles: "大文件",
  temporaryFiles: "临时文件",
  archiveFiles: "压缩包 / 安装包",
  emptyFiles: "空文件",
  emptyDirs: "空目录",
  keep: "保留",
  review: "待审",
  deleteAdvice: "建议删除",
  moveAdvice: "建议移动",
  duplicate: "重复",
  aiSummary: "AI 摘要",
  aiInspect: "AI 解读",
  aiInspectTitle: "路径 AI 解读",
  aiInspectHint:
    "支持对单个文件或目录调用 AI。目录分析只会上传文件名、扩展名、大小、数量等摘要信息，不会读取整个目录下所有文件内容，用来减少 token 消耗。",
  aiInspectFailed: "路径 AI 解读失败",
  aiInspectRemoteSuccess: "已成功调用远程 AI 模型",
  aiInspectFallbackTitle: "远程 AI 调用失败，已回退本地规则",
  aiInspectLocalOnlyTitle: "当前使用本地规则分析",
  aiInspectFallbackReason: "回退原因",
  aiInspectReason: "处理建议说明",
  aiInspectSummary: "AI 解读结论",
  aiInspectLoading: "正在分析这个路径，请稍候...",
  aiInspectTargetFile: "文件",
  aiInspectTargetDirectory: "目录",
  aiRiskLow: "低风险",
  aiRiskMedium: "中风险",
  aiRiskHigh: "高风险",
  close: "关闭",
  retry: "重新分析",
  goCleanup: "前往清理",
  groupPrefix: "第",
  groupSuffix: "组",
  duplicateFiles: "重复文件",
  moduleDescDuplicate: "同内容文件组，适合人工确认后清理冗余副本",
  moduleDescLarge: "占用空间高的文件，优先人工评估",
  moduleDescTemporary: "中间态、缓存态或下载残留文件",
  moduleDescArchive: "压缩包、镜像和安装包，适合归档或复核",
  moduleDescEmptyFiles: "体积为 0 的文件",
  moduleDescEmptyDirs: "不包含任何内容的目录",
  emptyOverview: "当前扫描结果没有可展示的目录内容。",
  loadingDirectoryOverview: "正在加载目录概览...",
  loadingFileTree: "正在按当前筛选条件加载文件树...",
  directoryOverviewFailed: "目录概览加载失败",
  directoryTreeEmpty: "当前没有可展示的目录空间结果。",
  directoryTreeHint: "按目录占用空间降序展示，可继续展开查看子目录和更深层级。",
  fileTreeFailed: "文件清单加载失败",
  fileTreeEmpty: "当前筛选条件下没有匹配文件。",
  fileTreeTruncated: "匹配结果过多，当前仅展示前 3000 个匹配文件以避免页面卡死。",
  fileTreeCollapsed: "结果较多，目录默认折叠显示，避免界面卡顿。",
  fileTreeDefaultMode: "当前默认先展示顶层目录/文件概览。输入关键词、选择类型或点击应用卡片后，会切换到更细的详细树。",
  explorerTitle: "资源浏览器",
  explorerHint: "在同一张卡片里查看目录、子目录和子文件。默认目录全部收起，展开后按空间占用大小排序。",
  visialeExplorerTitle: "可视化资源浏览器",
  multiChoice:"多项选择",
  multiDarg:"拖动",
  clickToExpand: "展开/收起文件夹",
  clickToDig:"进入文件夹",
  toParentPath: "返回上级目录",
  toChoiceAll: "全选",
  toChoiceInvert:"反选",
  toChoiceNull:"空选",
  toPackAll:"收起全部",
  toAiAdvise:"所选项ai分析",
  suggestionsLimited: "建议列表已做截断，仅展示前 1000 条建议。",
  duplicateGroupsLimited: "重复文件组已做截断，仅展示前 10 组。",
  sectionLimited: "当前结果页仅展示前 50 项，避免大盘扫描导致页面占用过高。",
  dedupPendingTitle: "重复文件识别仍在后台继续",
  dedupPendingDefault: "基础结果已经可以查看，重复文件识别完成后会自动刷新到当前页面。",
  dedupFailedTitle: "重复文件后台识别失败",
};

use([PieChart, TreemapChart, TitleComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

const router = useRouter();
const store = useAppStore();
const { requestFileInsight } = useAiFile();
const { getLatestScanReport } = useScan();
const message = useMessage();
const report = computed(() => store.report);
const reportKey = computed(() =>
  report.value ? `${report.value.generatedAt}:${report.value.root}` : ""
);

const fileQuery = ref("");
const selectedCategory = ref<FileCategory>("all");
const selectedApp = ref<AppOverviewRow | null>(null);
const aiInsightVisible = ref(false);
const selectedAiPath = ref("");
const fileAiInsightCache = ref<Record<string, FileAiInsight>>({});
const fileAiInsightPending = ref<Record<string, boolean>>({});
const fileAiInsightErrors = ref<Record<string, string>>({});

const fileTreeResult = ref<FileTreeQueryResult>(emptyFileTreeResult());
const fileTreeLoading = ref(false);
const fileTreeError = ref<string | null>(null);
const directoryTreeResult = ref<FileTreeQueryResult>(emptyFileTreeResult());
const directoryTreeLoading = ref(false);
const directoryTreeError = ref<string | null>(null);

let fileTreeTimer: ReturnType<typeof setTimeout> | null = null;
let backgroundRefreshTimer: ReturnType<typeof setInterval> | null = null;
let directoryTreeTimer: ReturnType<typeof setTimeout> | null = null;
let fileTreeRequestId = 0;
let directoryTreeRequestId = 0;
const hasShownDedupReadyToast = ref(false);

const selectedAiInsight = computed(() =>
  selectedAiPath.value ? fileAiInsightCache.value[selectedAiPath.value] ?? null : null
);
const selectedAiInsightPending = computed(() =>
  selectedAiPath.value ? Boolean(fileAiInsightPending.value[selectedAiPath.value]) : false
);
const selectedAiInsightError = computed(() =>
  selectedAiPath.value ? fileAiInsightErrors.value[selectedAiPath.value] ?? null : null
);

const suggestionByPath = computed(() => {
  const map = new Map<string, FileSuggestion>();
  for (const item of report.value?.advisor.suggestions ?? []) {
    map.set(item.path, item);
  }
  return map;
});

const governanceSuggestions = computed<GovernanceSuggestion[]>(() =>
  report.value?.advisor.governanceSuggestions ?? []
);

const duplicateGroupCount = computed(
  () => report.value?.dedup.groupCount ?? report.value?.dedup.groups.length ?? 0
);
const scanDurationLabel = computed(() => formatDurationMs(report.value?.scanDurationMs ?? 0));

const moduleCards = computed(() =>
  (report.value?.modules ?? []).map((item: ScanModuleSummary) => ({
    ...item,
    label: moduleLabel(item.kind),
    description: moduleDescription(item.kind),
  }))
);

const typeChartOption = computed(() => {
  if (!report.value) return {};
  const breakdown = report.value.analysis.typeBreakdown.slice(0, 10);
  return {
    tooltip: { trigger: "item", formatter: "{b}: {d}%" },
    legend: { orient: "vertical", right: 10, top: 20 },
    series: [
      {
        name: TEXT.fileType,
        type: "pie",
        radius: ["40%", "70%"],
        avoidLabelOverlap: false,
        itemStyle: { borderRadius: 6, borderColor: "#fff", borderWidth: 2 },
        label: { show: false },
        emphasis: { label: { show: true, fontSize: 14, fontWeight: "bold" } },
        data: breakdown.map((item) => ({
          name: item.extension || TEXT.noExt,
          value: item.totalSize,
        })),
      },
    ],
  };
});

const fileCategoryOptions = [
  { label: TEXT.categoryAll, value: "all" },
  { label: TEXT.categoryImage, value: "image" },
  { label: TEXT.categoryVideo, value: "video" },
  { label: TEXT.categoryAudio, value: "audio" },
  { label: TEXT.categoryArchive, value: "archive" },
  { label: TEXT.categoryExecutable, value: "executable" },
  { label: TEXT.categoryDocument, value: "document" },
  { label: TEXT.categoryCode, value: "code" },
  { label: TEXT.categoryOther, value: "other" },
];

const fileTreeRows = computed(() => fileTreeResult.value.rows);
const directoryTreeRows = computed(() => directoryTreeResult.value.rows);
const explorerRows = computed(() =>
  isDefaultFileTreeMode.value ? directoryTreeRows.value : fileTreeRows.value
);
const explorerLoading = computed(() =>
  isDefaultFileTreeMode.value ? directoryTreeLoading.value : fileTreeLoading.value
);
const explorerError = computed(() =>
  isDefaultFileTreeMode.value ? directoryTreeError.value : fileTreeError.value
);
const explorerMatchedCount = computed(() =>
  isDefaultFileTreeMode.value ? directoryTreeResult.value.matchedCount : fileTreeResult.value.matchedCount
);
const explorerNodeCount = computed(() =>
  isDefaultFileTreeMode.value ? directoryTreeResult.value.nodeCount : fileTreeResult.value.nodeCount
);

const isChartFullscreen = ref(false);
const chartRef = ref<any>(null);

function toggleFullscreen() {
  isChartFullscreen.value = !isChartFullscreen.value;
  
  nextTick(() => {
    if (chartRef.value) {
      chartRef.value.resize();
    }
  });
}

interface RawNode {
  name: string;
  value: number;
  path: string[];
  kind: "dir"|"file";
  load?: "undo"|"wait"|"done";
  hidden: boolean;
  chioced?: boolean;
  itemStyle?: any;
  upperLable?:any;
  children?: RawNode[];
  aiStatu?: "undo"|"wait"|"done",
  aiBrief?: string,
  aiRank?: "low" | "medium" | "high",
  aiReson?:string,
}

// const viewRawData = ref<RawNode[]>([
//   {
//     name:'.',
//     value: 1500,
//     path:[],
//     hidden:false,
//     chioced:false,
//     children:[
//       {
//         name: 'Folder A',
//         value: 1000,
//         path: ['.'],
//         hidden: false,
//         chioced:false,
//         children: [
//           {
//             name: 'File A1',
//             value: 400,
//             path: ['.','Folder A'],
//             hidden: false,
//             chioced:false,
//           },
//           { 
//             name: 'File A2',
//             value: 600, 
//             path: ['.','Folder A'],
//             hidden: false,
//             chioced:false,
//           }
//         ]
//       },
//       {
//         name: 'Folder B',
//         value: 500,
//         path: ['.'],
//         hidden: false,
//         chioced:false,
//         children: [
//           {
//             name: 'File B1', 
//             value: 200, 
//             path: ['.','Folder B'], 
//             hidden: false,
//             chioced:false,
//           },
//           { 
//             name: 'File B2', 
//             value: 300, 
//             path: ['.','Folder B'],
//             hidden: false,
//             chioced:false,
//           }
//         ]
//       }
//     ]
//   }
// ]);


const viewRawData = ref<RawNode[]>([
  {
    name:'.',
    value: 0,
    path:[],
    hidden:false,
    chioced:false,
    kind:"dir",
  }]
)
const viewRootPath = ref(['.']);
const chartClickMode = ref('expand');
const choicedFilePath = ref(new Set<string>());

async function loadChildrenForNode(node: RawNode) {
  if (!report.value) {
    message.error('没有扫描报告');
    return;
  }
  
  try {
    const relativePath = [...node.path,node.name].join('/').slice(2);
    const children = await invoke<FileTreeRow[]>('load_directory_children', {
      dirPath: relativePath
    });
    
    function convertToRawNode(row: FileTreeRow): RawNode {
      const newPath = [...node.path,node.name];
      
      return {
        name: row.name,
        value: Math.round(row.size / 1024),
        path: newPath,
        kind: row.kind === 'directory' ? 'dir' : 'file',
        load: row.kind === 'directory' ? 'undo' : 'done',
        hidden: true,
        chioced: false,
        children: undefined,
      };
    }
    
    node.children = children.map(convertToRawNode);
    message.success(`已加载 ${children.length} 个子项`);
    node.load = 'done';
  } catch (error) {
    message.error(`加载子节点失败：${error}`);
    node.load = 'undo';
  }
}

async function loadChildrenInit() {
  if (!report.value) {
    message.error('没有扫描报告');
    return;
  }
  
  try {
    const children = await invoke<FileTreeRow[]>('load_directory_children', {
      dirPath: ''
    });
    
    function convertToRawNode(row: FileTreeRow): RawNode {
      const newPath = ['.'];
      
      return {
        name: row.name,
        value: Math.round(row.size / (1024)),
        path: newPath,
        kind: row.kind === 'directory' ? 'dir' : 'file',
        load: row.kind === 'directory' ? 'undo' : 'done',
        hidden: true,
        chioced: false,
        children: undefined,
      };
    }
    
    viewRawData.value[0].children = children.map(convertToRawNode);
  } catch (error) {
    message.error(`加载子节点失败：${error}`);
  }
}

onMounted(()=>{
  void loadChildrenInit();
});


function getNodeByPath(path:string[]){ 
  if(!path||path.length<1)
  {
    console.log('path is null');
    return null;
  }
  let node:RawNode[]=viewRawData.value;
  for(let i=0;i<path.length;i++)
  {
    let find=node.find(n=>n.name==path[i]);
    if(find==null)
    {
      console.log(`not find node ${path[i]}`);
      return null;
    }
    if(i<path.length-1)
    {
      if(find.children){
        node=find.children;
      }        
    }
    else{return find;}
  }
  return null;
}

function handleChartClick(params: any) {
  if(chartClickMode.value=='expand')
  {
    const path=[...params.data.path, params.data.name];
    let node=getNodeByPath(path);
    
    if(node==null)
    {
      console.log('node is null');
      return;
    }
    
    node.hidden=!(node.hidden ?? false);
    
    if(!node.hidden && node.kind === 'dir' && (node?.load??"undo")==="undo" && !node.children)
    {
      node.load="wait";
      void loadChildrenForNode(node);
    }
  }
  else if(chartClickMode.value=='dig')
  {
    if(params.data.children && params.data.children.length > 0)
    {
      viewRootPath.value = [...params.data.path, params.data.name]; 
    }
  }
  else if(chartClickMode.value=='multi')
  {
    const path = [...params.data.path, params.data.name];
    const pathKey = path.join('/');
    let node = getNodeByPath(path);
    
    if(node == null) {
      console.log('node is null');
      return;
    }
    
    if(choicedFilePath.value.has(pathKey)) {
      choicedFilePath.value.delete(pathKey);
      node.chioced = false;
    } else {
      choicedFilePath.value.add(pathKey);
      node.chioced = true;
    }
  }
}

function handleToParentPath() {
  if (viewRootPath.value.length>1)
  {
    viewRootPath.value=viewRootPath.value.slice(0, -1);
  }
}

function choiceAllSub(path:string[],mode:"choice"|"invert"|"unchoice"){
  let node=getNodeByPath(path);
  if(node==null) return;
  if(node?.children==null) return;
  if(node.children.length==0) return;
  for(let i=0;i<node.children.length;i++)
  {
    if(mode=="choice")
    {
      node.children[i].chioced=true;
      choicedFilePath.value.add([...node.children[i].path,node.children[i].name].join('/'))      
    }
    else if(mode=="invert")
    {
      if(node.children[i]?.chioced??false)
      {
        node.children[i].chioced=false;
        choicedFilePath.value.delete([...node.children[i].path,node.children[i].name].join('/'));
      }
      else
      {
        node.children[i].chioced=true;
        choicedFilePath.value.add([...node.children[i].path,node.children[i].name].join('/'));
      }
    }
    else if(mode=="unchoice")
    {
      node.children[i].chioced=false;
      choicedFilePath.value.delete([...node.children[i].path,node.children[i].name].join('/'))   
    }
    if(node.children[i].hidden==false)
    {
      let path=[...node.children[i].path,node.children[i].name].join('/');
      choiceAllSub([...node.children[i].path,node.children[i].name],mode);
    }
  }
}

function handleToChoiceAll(){
  choiceAllSub(viewRootPath.value,"choice");
}

function handleToChoiceInvert()
{
  choiceAllSub(viewRootPath.value,"invert");
}

function handleToChoiceNull()
{
  choiceAllSub(viewRootPath.value,"unchoice")
}

function packAllSub(path:string[])
{
  let node=getNodeByPath(path);
  if(node==null) return;
  if(node?.children==null) return;
  if(node.children.length==0) return;
  for(let i=0;i<node.children.length;i++)
  {
    node.children[i].hidden=true;   
    packAllSub([...node.children[i].path,node.children[i].name]);
  }
}
function handleToPackAll() {
  packAllSub(viewRootPath.value);
}

function buildViewData(node:RawNode[])
{
  const isDark = store.theme === 'dark';
  const backgroundColor = isDark ? '#0f172a' : '#f4f7fb';
  const anotherBackgroundColor = isDark ? '#f4f7fb' : '#0f172a';
  const borderColor = isDark ? '#334155' : '#e6ebf3';
  const anotherBorderColor = isDark ? '#e6ebf3' : '#334155';
  const textColor = isDark ? '#cbd5e1' : '#516079';
  const anotherTextColor = isDark ? '#516079' : '#cbd5e1';
  const selectedBorderColor = isDark ? '#3b82f6' : '#2563eb';
  // AI 风险等级对应的颜色方案
  const aiRiskColors = {
    low: {
      bg: isDark ? '#86efac' : '#86efac',
    },
    medium: {
      bg: isDark ? '#fde047' : '#fde047', 
    },
    high: {
      bg: isDark ? '#fca5a5' : '#fca5a5',
    },
  };
  return node.map(n => {
    const useAiColor = (n?.aiStatu ?? 'undo') === 'done' && n.aiRank && aiRiskColors[n.aiRank];
    let itemBgColor: string;
    let itemBorderColor: string;
    let labelColor: string;
    let labelBgColor: string;
    if(useAiColor)
    {
      const colors = aiRiskColors[n.aiRank!];
      labelColor = "#516079";
      labelBgColor = colors.bg;
    }
    else 
    {
      labelColor = textColor;
      labelBgColor = backgroundColor;
    }

    if(n?.chioced??false)
    {
      itemBgColor = selectedBorderColor;
      itemBorderColor = selectedBorderColor;
    }
    else 
    {
      const isEven = n.path.length % 2 === 0;
      itemBgColor = isEven ? backgroundColor : anotherBackgroundColor;
      itemBorderColor = isEven ? borderColor : anotherBorderColor;
    }

    const newNode: any = {
      ...n,
      children: n.hidden ? undefined : buildViewData(n.children ?? []),
      itemStyle: {
        borderColor: itemBorderColor,
        backgroundColor: itemBgColor,
      },
      label: {
        color: labelColor,
        backgroundColor: labelBgColor,
      },
      // upperLabel: {
      //   height: 28,
      //   fontSize: 12,
      //   color: n.path.length%2==0 ? textColor : anotherTextColor,
      //   backgroundColor: 'transparent'
      // }
    };
    return newNode;
  });
}

async function singleAiAdvise(node:RawNode)
{
  if (!report.value) {
    message.error('没有扫描报告');
    return;
  }
  let relativePath=[...node.path.slice(1),node.name].join('\\');
  const fullPath = report.value.root + (relativePath ? '\\' + relativePath : '');
  console.log(fullPath);
  if(node==null)return;
  if(node?.aiStatu??"undo"!="undo")return;
  node.aiStatu="wait";
  try{
    const insight = await invoke<FileAiInsight>("explain_file_with_ai",{path:fullPath});
    node.aiBrief=insight.summary;
    node.aiReson=insight.reason;
    node.aiRank=insight.risk;
    node.aiStatu="done";
    console.log(node);
  }
  catch(error){
    message.error(`ai解读失败：${error}`);
    node.aiStatu="undo";
  }
  
}
async function choiceAiAdvise() {
  if (choicedFilePath.value.size === 0) {
    message.warning('请先选择要分析的文件或目录');
    return;
  }
  
  const paths = Array.from(choicedFilePath.value);
  message.info(`开始对 ${paths.length} 个选中项进行 AI 分析...`);
  for(const pathKey of paths)
  {
    const pathArray = pathKey.split('/');
    let node = getNodeByPath(pathArray);
    if(node == null) {
      console.warn(`未找到节点: ${pathKey}`);
      continue;
    }
    node.chioced = false;
  }
  for(const pathKey of paths) {
    try {
      const pathArray = pathKey.split('/');
      let node = getNodeByPath(pathArray);
      if (node == null) {
        continue;
      }
      singleAiAdvise(node);
      await new Promise(resolve => setTimeout(resolve, 10));
    } catch (error) {
      console.error(`处理路径 ${pathKey} 时出错:`, error);
    }
  }
  choicedFilePath.value.clear();
}

const computedViewData = computed(() => {
  let rootnode=getNodeByPath(viewRootPath.value);
  return buildViewData(rootnode?.children??[]);
})

const chartOption = computed(() => {
  const isDark = store.theme === 'dark';
  const backgroundColor = isDark ? '#0f172a' : '#f4f7fb';
  const borderColor = isDark ? '#334155' : '#e6ebf3';
  const textColor = isDark ? '#cbd5e1' : '#516079';
  return {
    tooltip: {
      confine: true,
      extraCssText: `max-width: 400px; white-space: normal; word-wrap: break-word; word-break: break-word; background-color: ${backgroundColor};`,
      formatter: (params: any) => {
        if((params?.data?.aiStatu??"undo")!="done")
        {
          return `
          <div style="max-width: 380px; word-wrap: break-word; word-break: break-word;">
            <strong style="display: block; margin-bottom: 4px; color: ${textColor};">${params.name}</strong>
            <span style="color: ${textColor};">大小: ${params.value} KB</span>
          </div>`;
        }
        else
        {
          return `
          <div style="max-width: 380px; word-wrap: break-word; word-break: break-word;">
            <strong style="display: block; margin-bottom: 6px; color: ${textColor}; word-break: break-all;">${params.name}</strong>
            <div style="margin-bottom: 8px; color: ${textColor};">大小: ${params.value} KB</div>
            <div style="margin-top: 8px; padding-top: 8px; border-top: 1px solid ${borderColor};">
              <div style="color: ${textColor}; font-size: 12px; margin-bottom: 4px;">删除风险 AI 评级：</div>
              <div style="color: ${textColor}; line-height: 1.6; word-wrap: break-word; word-break: break-word; white-space: pre-wrap;">${params?.data?.aiRank??'暂无'}</div>
            </div>
            <div style="margin-top: 8px; padding-top: 8px; border-top: 1px solid ${borderColor};">
              <div style="color: ${textColor}; font-size: 12px; margin-bottom: 4px;">AI 总结：</div>
              <div style="color: ${textColor}; line-height: 1.6; word-wrap: break-word; word-break: break-word; white-space: pre-wrap;">${params?.data?.aiBrief??'暂无'}</div>
            </div>
            <div style="margin-top: 8px; padding-top: 8px; border-top: 1px solid ${borderColor};">
              <div style="color: ${textColor}; font-size: 12px; margin-bottom: 4px;">AI 详解：</div>
              <div style="color: ${textColor}; line-height: 1.6; word-wrap: break-word; word-break: break-word; white-space: pre-wrap;">${params?.data?.aiReson??'暂无'}</div>
            </div>
          </div>`;
        }
        
      },
    },
    series: [
      {
        type: 'treemap',
        animation: false,
        data: computedViewData.value,
        width: '100%',
        height: '100%',
        nodeClick: false,
        breadcrumb: { show: false },
        itemStyle: {
          borderWidth: 3,
          gapWidth: 3,
        },
        upperLabel: {
          show: true,
          height: 28,
          fontSize: 12,
          backgroundColor: 'transparent',
        },
        label: {
          show: true,
          fontSize: 11,
          formatter: '{b} {c}KB',
        },

        levels: [
          {
            itemStyle: { borderWidth: 0, gapWidth: 0 },
            upperLabel: { show: false },
            label: { show: false },
          }
        ],
        leafDepth: 1000,
      },
    ],
  };
});

const scannedFilesHint = computed(() => {
  if (!report.value) return "";
  return `${TEXT.scannedFilesHintPrefix}${explorerMatchedCount.value}${TEXT.scannedFilesHintMiddle}${explorerNodeCount.value}${TEXT.scannedFilesHintSuffix}`;
});
const shouldExpandFileTree = computed(() => fileTreeResult.value.nodeCount <= 200);
const isDefaultFileTreeMode = computed(
  () => !fileQuery.value.trim() && selectedCategory.value === "all" && !selectedApp.value
);
const explorerEmptyText = computed(() =>
  isDefaultFileTreeMode.value ? TEXT.directoryTreeEmpty : TEXT.fileTreeEmpty
);
const normalizedSelectedAppRoot = computed(() =>
  selectedApp.value ? normalizePath(selectedApp.value.detectedRoot) : ""
);

const filteredLargeFiles = computed(() =>
  filterFilesBySelectedApp(report.value?.analysis.largeFiles ?? [])
);
const filteredTemporaryFiles = computed(() =>
  filterFilesBySelectedApp(report.value?.analysis.temporaryFiles ?? [])
);
const filteredArchiveFiles = computed(() =>
  filterFilesBySelectedApp(report.value?.analysis.archiveFiles ?? [])
);
const filteredDedupGroups = computed(() =>
  filterDuplicateGroupsBySelectedApp(report.value?.dedup.groups ?? [])
);

const directoryTreeColumns: DataTableColumns<FileTreeRow> = [
  {
    title: TEXT.itemName,
    key: "name",
    ellipsis: { tooltip: true },
    render: (row) =>
      h("div", { style: "display: flex; flex-direction: column; gap: 2px;" }, [
        h(
          "div",
          { style: "display: flex; align-items: center; gap: 8px;" },
          [
            h(NTag, { size: "small", type: "info" }, () => TEXT.directory),
            h("span", row.name),
          ]
        ),
        h(NText, { depth: 3, style: "font-size: 12px;" }, () => row.path),
      ]),
  },
  {
    title: TEXT.size,
    key: "size",
    width: 120,
    sorter: (left, right) => left.size - right.size,
    render: (row) => formatBytes(row.size),
  },
  {
    title: TEXT.fileCount,
    key: "fileCount",
    width: 110,
    sorter: (left, right) => left.fileCount - right.fileCount,
  },
];

const fileTreeColumns: DataTableColumns<FileTreeRow> = [
  {
    title: TEXT.itemName,
    key: "name",
    ellipsis: { tooltip: true },
    render: (row) =>
      h("div", { style: "display: flex; flex-direction: column; gap: 2px;" }, [
        h("div", { style: "display: flex; align-items: center; gap: 8px;" }, [
          h(
            NTag,
            {
              size: "small",
              type: row.kind === "directory" ? "info" : "default",
            },
            () => (row.kind === "directory" ? TEXT.directory : TEXT.file)
          ),
          h("span", row.name),
        ]),
        h(
          NText,
          {
            depth: 3,
            style: "font-size: 12px;",
          },
          () => row.path
        ),
      ]),
  },
  {
    title: TEXT.size,
    key: "size",
    width: 120,
    sorter: (left, right) => left.size - right.size,
    render: (row) => formatBytes(row.size),
  },
  {
    title: TEXT.ext,
    key: "extension",
    width: 110,
    render: (row) => (row.kind === "directory" ? "-" : row.extension || "-"),
  },
  {
    title: TEXT.fileCount,
    key: "fileCount",
    width: 90,
    render: (row) => (row.kind === "directory" ? row.fileCount : "-"),
  },
  {
    title: TEXT.aiInspect,
    key: "aiInspect",
    width: 110,
    render: (row) =>
      h(
        NButton,
        {
          size: "tiny",
          secondary: true,
          type: "primary",
          loading: Boolean(fileAiInsightPending.value[row.path]),
          onClick: () => void handleFileAiAction(row.path),
        },
        () => fileAiButtonText(row.path)
      ),
  },
];

const fileColumns: DataTableColumns<FileRecord> = [
  {
    title: TEXT.filePath,
    key: "path",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.size,
    key: "size",
    width: 120,
    sorter: (left, right) => left.size - right.size,
    render: (row) => formatBytes(row.size),
  },
  {
    title: TEXT.ext,
    key: "extension",
    width: 110,
    render: (row) => row.extension || "-",
  },
  {
    title: TEXT.aiInspect,
    key: "aiInspect",
    width: 110,
    render: (row) =>
      h(
        NButton,
        {
          size: "tiny",
          secondary: true,
          type: "primary",
          loading: Boolean(fileAiInsightPending.value[row.path]),
          onClick: () => void handleFileAiAction(row.path),
        },
        () => fileAiButtonText(row.path)
      ),
  },
];

watch(
  reportKey,
  async (key) => {
    if (!key) {
      resetFileTreeState();
      resetDirectoryTreeState();
      stopBackgroundRefresh();
      hasShownDedupReadyToast.value = false;
      return;
    }
    scheduleExplorerLoad(0);
    ensureBackgroundRefresh();
  },
  { immediate: true }
);

watch([fileQuery, selectedCategory], () => {
  if (!reportKey.value) {
    return;
  }
  scheduleExplorerLoad(250);
});

watch(
  () => selectedApp.value?.key,
  () => {
    if (!reportKey.value) {
      return;
    }
    scheduleExplorerLoad(0);
  }
);

onBeforeUnmount(() => {
  if (fileTreeTimer) {
    clearTimeout(fileTreeTimer);
    fileTreeTimer = null;
  }
  if (directoryTreeTimer) {
    clearTimeout(directoryTreeTimer);
    directoryTreeTimer = null;
  }
  stopBackgroundRefresh();
});

watch(
  () => report.value?.dedupPending,
  (pending) => {
    if (pending) {
      hasShownDedupReadyToast.value = false;
      ensureBackgroundRefresh();
    } else {
      stopBackgroundRefresh();
    }
  },
  { immediate: true }
);

function emptyFileTreeResult(): FileTreeQueryResult {
  return {
    matchedCount: 0,
    nodeCount: 0,
    truncated: false,
    rows: [],
  };
}

function resetFileTreeState() {
  fileTreeResult.value = emptyFileTreeResult();
  fileTreeLoading.value = false;
  fileTreeError.value = null;
}

function resetDirectoryTreeState() {
  directoryTreeResult.value = emptyFileTreeResult();
  directoryTreeLoading.value = false;
  directoryTreeError.value = null;
}

function stopBackgroundRefresh() {
  if (backgroundRefreshTimer) {
    clearInterval(backgroundRefreshTimer);
    backgroundRefreshTimer = null;
  }
}

function ensureBackgroundRefresh() {
  stopBackgroundRefresh();
  if (!report.value?.dedupPending) {
    return;
  }
  backgroundRefreshTimer = setInterval(() => {
    void refreshLatestReport();
  }, 1500);
}

async function refreshLatestReport() {
  const wasPending = Boolean(report.value?.dedupPending);
  const latest = await getLatestScanReport();
  if (!latest) {
    return;
  }
  store.setReport(latest);
  if (!latest.dedupPending) {
    stopBackgroundRefresh();
    if (wasPending && !latest.dedupError && !hasShownDedupReadyToast.value) {
      hasShownDedupReadyToast.value = true;
      message.success("重复文件结果已补充完成。");
    }
  }
}

function scheduleExplorerLoad(delayMs: number) {
  if (isDefaultFileTreeMode.value) {
    scheduleDirectoryTreeLoad(delayMs);
    return;
  }
  scheduleFileTreeLoad(delayMs);
}

function scheduleDirectoryTreeLoad(delayMs: number) {
  if (directoryTreeTimer) {
    clearTimeout(directoryTreeTimer);
  }
  directoryTreeTimer = setTimeout(() => {
    directoryTreeTimer = null;
    void loadDirectoryTree();
  }, delayMs);
}

function scheduleFileTreeLoad(delayMs: number) {
  if (fileTreeTimer) {
    clearTimeout(fileTreeTimer);
  }
  fileTreeTimer = setTimeout(() => {
    fileTreeTimer = null;
    void loadFileTree();
  }, delayMs);
}

async function loadFileTree() {
  const requestId = ++fileTreeRequestId;
  fileTreeLoading.value = true;
  fileTreeError.value = null;

  try {
    const appQuery = selectedApp.value?.detectedRoot?.trim() || "";
    const keywordQuery = fileQuery.value.trim();
    const result = await invoke<FileTreeQueryResult>("query_file_tree_v2", {
      query: appQuery || keywordQuery || null,
      category: selectedCategory.value,
    });
    if (requestId !== fileTreeRequestId) {
      return;
    }
    fileTreeResult.value = result;
  } catch (error) {
    if (requestId !== fileTreeRequestId) {
      return;
    }
    fileTreeResult.value = emptyFileTreeResult();
    fileTreeError.value =
      typeof error === "string" ? error : (error as Error).message || String(error);
  } finally {
    if (requestId === fileTreeRequestId) {
      fileTreeLoading.value = false;
    }
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatDurationMs(durationMs: number): string {
  if (durationMs < 1000) return `${durationMs} ms`;

  const totalSeconds = Math.floor(durationMs / 1000);
  if (totalSeconds < 60) return `${totalSeconds} 秒`;

  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) return seconds > 0 ? `${minutes} 分 ${seconds} 秒` : `${minutes} 分`;

  const hours = Math.floor(minutes / 60);
  const remainMinutes = minutes % 60;
  if (remainMinutes > 0) return `${hours} 小时 ${remainMinutes} 分`;
  return `${hours} 小时`;
}

function moduleLabel(kind: ScanModuleKind): string {
  if (kind === "duplicate_files") return TEXT.duplicateFiles;
  if (kind === "large_files") return TEXT.largeFiles;
  if (kind === "temporary_files") return TEXT.temporaryFiles;
  if (kind === "archive_files") return TEXT.archiveFiles;
  if (kind === "empty_files") return TEXT.emptyFiles;
  return TEXT.emptyDirs;
}

function moduleDescription(kind: ScanModuleKind): string {
  if (kind === "duplicate_files") return TEXT.moduleDescDuplicate;
  if (kind === "large_files") return TEXT.moduleDescLarge;
  if (kind === "temporary_files") return TEXT.moduleDescTemporary;
  if (kind === "archive_files") return TEXT.moduleDescArchive;
  if (kind === "empty_files") return TEXT.moduleDescEmptyFiles;
  return TEXT.moduleDescEmptyDirs;
}

function goToCleanup() {
  router.push({ name: "cleanup" });
}

async function loadDirectoryTree() {
  const requestId = ++directoryTreeRequestId;
  directoryTreeLoading.value = true;
  directoryTreeError.value = null;

  try {
    const appQuery = selectedApp.value?.detectedRoot?.trim() || "";
    const result = await invoke<FileTreeQueryResult>("query_directory_tree_v2", {
      query: appQuery || null,
    });
    if (requestId !== directoryTreeRequestId) {
      return;
    }
    directoryTreeResult.value = result;
  } catch (error) {
    if (requestId !== directoryTreeRequestId) {
      return;
    }
    directoryTreeResult.value = emptyFileTreeResult();
    directoryTreeError.value =
      typeof error === "string" ? error : (error as Error).message || String(error);
  } finally {
    if (requestId === directoryTreeRequestId) {
      directoryTreeLoading.value = false;
    }
  }
}

function handleSelectApp(app: AppOverviewRow) {
  selectedApp.value = app;
}

function handleClearSelectedApp() {
  selectedApp.value = null;
}

function normalizePath(value: string): string {
  return value.replace(/\\/g, "/").toLowerCase();
}

function matchesSelectedAppPath(path: string): boolean {
  if (!normalizedSelectedAppRoot.value) {
    return true;
  }
  return normalizePath(path).startsWith(normalizedSelectedAppRoot.value);
}

function filterFilesBySelectedApp(files: FileRecord[]): FileRecord[] {
  return files.filter((file) => matchesSelectedAppPath(file.path));
}

function filterDuplicateGroupsBySelectedApp(groups: DuplicateGroup[]): DuplicateGroup[] {
  if (!normalizedSelectedAppRoot.value) {
    return groups;
  }
  return groups.filter((group) =>
    group.files.some((file) => matchesSelectedAppPath(file.path))
  );
}

function fileAiButtonText(path: string): string {
  if (fileAiInsightPending.value[path]) {
    return "解读中";
  }
  if (fileAiInsightCache.value[path]) {
    return "查看解读";
  }
  if (fileAiInsightErrors.value[path]) {
    return "重试解读";
  }
  return TEXT.aiInspect;
}

async function queueFileInsight(path: string, force = false) {
  if (fileAiInsightPending.value[path]) {
    return;
  }
  if (!force && fileAiInsightCache.value[path]) {
    return;
  }

  fileAiInsightPending.value = {
    ...fileAiInsightPending.value,
    [path]: true,
  };

  const nextErrors = { ...fileAiInsightErrors.value };
  delete nextErrors[path];
  fileAiInsightErrors.value = nextErrors;

  try {
    const result = await requestFileInsight(path, store.config);
    fileAiInsightCache.value = {
      ...fileAiInsightCache.value,
      [path]: result,
    };
  } catch (error: any) {
    fileAiInsightErrors.value = {
      ...fileAiInsightErrors.value,
      [path]: typeof error === "string" ? error : error?.message || String(error),
    };
  } finally {
    fileAiInsightPending.value = {
      ...fileAiInsightPending.value,
      [path]: false,
    };
  }
}

async function handleFileAiAction(path: string) {
  selectedAiPath.value = path;

  if (fileAiInsightCache.value[path] || fileAiInsightErrors.value[path]) {
    aiInsightVisible.value = true;
    return;
  }

  if (fileAiInsightPending.value[path]) {
    message.info("这个项目正在后台解读，稍后再点查看结果。");
    return;
  }

  // 检查是否配置了 API Key
  const hasApiKey = Boolean(store.config?.apiKey?.trim());
  if (!hasApiKey) {
    message.info("AI 解读需要先配置 API Key，请先去设置页面配置。");
    return;
  }

  void queueFileInsight(path);
  message.info("已开始后台解读，你可以继续查看其它文件或目录。");
}

async function retrySelectedFileInsight() {
  if (!selectedAiPath.value) {
    return;
  }
  void queueFileInsight(selectedAiPath.value, true);
  message.info("已重新加入后台解读队列。");
}

function formatSourceLabel(source?: string | null): string {
  if (!source) return "";
  if (source === "local_rules") return TEXT.localRules;
  if (source.startsWith("remote:")) {
    return `${TEXT.remoteModel}${source.slice("remote:".length)}`;
  }
  return source;
}

function actionTagType(
  action: SuggestedAction
): "default" | "success" | "warning" | "error" | "info" {
  if (action === "keep") return "success";
  if (action === "review") return "info";
  if (action === "move") return "warning";
  if (action === "delete") return "error";
  return "default";
}

function actionLabel(action: SuggestedAction): string {
  if (action === "keep") return TEXT.keep;
  if (action === "review") return TEXT.review;
  if (action === "move") return TEXT.moveAdvice;
  if (action === "delete") return TEXT.deleteAdvice;
  return action;
}

function riskTagType(risk: "low" | "medium" | "high"): "success" | "warning" | "error" {
  if (risk === "low") return "success";
  if (risk === "medium") return "warning";
  return "error";
}

function riskLabel(risk: "low" | "medium" | "high"): string {
  if (risk === "low") return TEXT.aiRiskLow;
  if (risk === "medium") return TEXT.aiRiskMedium;
  return TEXT.aiRiskHigh;
}

function aiTargetKindLabel(targetKind: FileAiInsight["targetKind"]): string {
  if (targetKind === "directory") return TEXT.aiInspectTargetDirectory;
  return TEXT.aiInspectTargetFile;
}

function normalizeAiText(value: string | null | undefined): string {
  return (value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[，。、“”‘’：；！？、,.!?:;"'()[\]{}\-_\s]/g, "");
}

function shouldShowSeparateReasonCard(insight: FileAiInsight): boolean {
  const normalizedSummary = normalizeAiText(insight.summary);
  const normalizedReason = normalizeAiText(insight.reason);

  if (!normalizedReason) return false;
  if (!normalizedSummary) return true;
  if (normalizedSummary === normalizedReason) return false;
  if (normalizedSummary.includes(normalizedReason)) return false;
  if (normalizedReason.includes(normalizedSummary)) return false;
  return true;
}

function duplicateTagType(path: string): "default" | "success" | "warning" | "error" | "info" {
  const action = suggestionByPath.value.get(path)?.action;
  if (action === "keep") return "success";
  if (action === "review") return "info";
  if (action === "move") return "warning";
  if (action === "delete") return "error";
  return "default";
}

function duplicateTagLabel(path: string): string {
  const action = suggestionByPath.value.get(path)?.action as SuggestedAction | undefined;
  if (action === "keep") return TEXT.keep;
  if (action === "review") return TEXT.review;
  if (action === "move") return TEXT.moveAdvice;
  if (action === "delete") return TEXT.deleteAdvice;
  return TEXT.duplicate;
}
</script>

<template>
  <div v-if="!report">
    <n-empty :description="TEXT.noScanResult">
      <template #extra>
        <n-button @click="router.push({ name: 'scan' })">{{ TEXT.goScan }}</n-button>
      </template>
    </n-empty>
  </div>

  <div v-else class="page-shell results-page">
    <section class="page-hero">
      <n-space vertical :size="18">
        <div>
          <div class="results-hero-kicker">Analysis Workbench</div>
          <div class="page-hero__title">扫描结果工作台</div>
          <div class="page-hero__desc">
            从应用、目录、文件和 AI 建议四个层级回看这次扫描结果。页面会把高价值信息放在前面，并支持继续联动到清理动作。
          </div>
        </div>
        <div class="soft-panel results-root-panel">
          <n-text depth="3">{{ TEXT.rootPath }}</n-text>
          <div class="results-root-value">{{ report.root }}</div>
        </div>
      </n-space>
    </section>

    <section class="metric-grid">
      <div class="metric-card">
        <div class="metric-card__label">{{ TEXT.totalFiles }}</div>
        <div class="metric-card__value">{{ report.analysis.totalFiles }}</div>
        <div class="metric-card__hint">本次纳入分析的文件数量</div>
      </div>
      <div class="metric-card">
        <div class="metric-card__label">{{ TEXT.totalSize }}</div>
        <div class="metric-card__value">{{ formatBytes(report.analysis.totalSize) }}</div>
        <div class="metric-card__hint">总扫描体积</div>
      </div>
      <div class="metric-card">
        <div class="metric-card__label">{{ TEXT.duplicateGroups }}</div>
        <div class="metric-card__value">{{ duplicateGroupCount }}</div>
        <div class="metric-card__hint">可进一步合并的重复内容</div>
      </div>
      <div class="metric-card">
        <div class="metric-card__label">扫描耗时</div>
        <div class="metric-card__value">{{ scanDurationLabel }}</div>
        <div class="metric-card__hint">用于衡量大盘扫描开销</div>
      </div>
    </section>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">治理建议主线</div>
            <div class="section-head__desc">
              用统一建议模型解释“为什么能动、为什么不能动、动之前要做什么”。
            </div>
          </div>
        </div>
      </template>

      <n-empty
        v-if="governanceSuggestions.length === 0"
        description="当前扫描结果还没有生成可展示的治理建议。"
      />
      <n-grid v-else cols="1 s:2" responsive="screen" :x-gap="12" :y-gap="12">
        <n-gi v-for="item in governanceSuggestions" :key="item.id">
          <div class="results-module-card">
            <div class="results-module-card__top">
              <div class="results-module-card__title">{{ item.title }}</div>
              <n-tag :type="item.riskLevel === 'high' ? 'error' : item.riskLevel === 'medium' ? 'warning' : 'success'" size="small">
                {{ item.riskLevel }}
              </n-tag>
            </div>
            <n-text depth="3">{{ item.summary }}</n-text>
            <div class="results-module-card__path">{{ item.targetPath }}</div>
            <div class="results-module-card__tags">
              <n-tag v-for="tag in item.tags" :key="`${item.id}-${tag}`" size="small" round>
                {{ tag }}
              </n-tag>
              <n-tag size="small" type="info" round>{{ item.action }}</n-tag>
            </div>
            <div class="results-module-card__checklist">
              <div class="results-module-card__check-title">执行前检查</div>
              <ul>
                <li v-for="check in item.preCheckItems" :key="check">{{ check }}</li>
              </ul>
            </div>
          </div>
        </n-gi>
      </n-grid>
    </n-card>

    <n-alert
      v-if="report.dedupPending"
      type="info"
      class="results-app-alert"
      :title="TEXT.dedupPendingTitle"
    >
      {{ report.dedupMessage || TEXT.dedupPendingDefault }}
    </n-alert>

    <n-alert
      v-if="report.dedupError"
      type="error"
      class="results-app-alert"
      :title="TEXT.dedupFailedTitle"
    >
      {{ report.dedupError }}
    </n-alert>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.explorerTitle }}</div>
            <div class="section-head__desc">{{ TEXT.explorerHint }}</div>
          </div>
        </div>
      </template>

      <n-space vertical :size="14">
        <div class="filter-bar results-filter-bar">
          <n-input
            v-model:value="fileQuery"
            clearable
            :placeholder="TEXT.fileSearchPlaceholder"
            class="results-filter-bar__search"
          />
          <n-select
            v-model:value="selectedCategory"
            :options="fileCategoryOptions"
            :placeholder="TEXT.fileCategoryPlaceholder"
            class="results-filter-bar__select"
          />
        </div>

        <n-alert v-if="selectedApp" type="info" class="results-app-alert">
          当前已按应用筛选：{{ selectedApp.appName }}（{{ selectedApp.detectedRoot }}）
        </n-alert>

        <n-alert
          v-if="explorerError"
          type="error"
          :title="isDefaultFileTreeMode ? TEXT.directoryOverviewFailed : TEXT.fileTreeFailed"
        >
          {{ explorerError }}
        </n-alert>
        <div v-else-if="explorerLoading" class="soft-panel">
          <n-text depth="3">
            {{ isDefaultFileTreeMode ? TEXT.loadingDirectoryOverview : TEXT.loadingFileTree }}
          </n-text>
        </div>

        <n-text v-if="!explorerError" depth="3">{{ scannedFilesHint }}</n-text>
        <n-alert v-if="isDefaultFileTreeMode && explorerRows.length > 0" type="info">
          {{ TEXT.directoryTreeHint }}
        </n-alert>
        <n-alert v-if="fileTreeResult.truncated" type="warning">
          {{ TEXT.fileTreeTruncated }}
        </n-alert>
        <n-alert v-if="!shouldExpandFileTree && explorerRows.length > 0" type="info">
          {{ TEXT.fileTreeCollapsed }}
        </n-alert>

        <n-data-table
          v-if="explorerRows.length > 0"
          :columns="fileTreeColumns"
          :data="explorerRows"
          :loading="explorerLoading"
          :max-height="520"
          size="small"
          :bordered="false"
          :default-expand-all="false"
        />
        <n-empty
          v-else-if="!explorerLoading && !explorerError"
          :description="explorerEmptyText"
        />
      </n-space>
    </n-card>

    <n-card class="surface-card interactive-card" :class="{ 'no-transition': isChartFullscreen }">
      <template #header>
        <div class="section-head">
          <span class="section-head__title">{{ TEXT.visialeExplorerTitle }}</span>
          <n-button @click="toggleFullscreen">
              {{ isChartFullscreen ? '退出全屏' : '全屏显示' }}
          </n-button>
        </div>
      </template>
      <div v-if="!isChartFullscreen" class="chart-container" style="height: 500px;">
          <div>
          <n-radio-group v-model:value="chartClickMode">
          <n-radio-button value="expand">{{ TEXT.clickToExpand }}</n-radio-button>
          <n-radio-button value="dig">{{ TEXT.clickToDig }}</n-radio-button>            
          <n-radio-button value="multi">{{ TEXT.multiChoice }}</n-radio-button>
          <n-radio-button value="darg">{{ TEXT.multiDarg }}</n-radio-button>
          </n-radio-group>
          <n-button @click="handleToParentPath">{{TEXT.toParentPath}}</n-button>
          <n-button @click="handleToChoiceAll">{{TEXT.toChoiceAll}}</n-button>
          <n-button @click="handleToChoiceInvert">{{TEXT.toChoiceInvert}}</n-button>
          <n-button @click="handleToChoiceNull">{{TEXT.toChoiceNull}}</n-button>
          <n-button @click="handleToPackAll">{{TEXT.toPackAll}}</n-button>
          <n-button @click="choiceAiAdvise">{{TEXT.toAiAdvise}}</n-button>
          </div>
        <v-chart
          ref="chartRef"
          :option="chartOption"
          @click="handleChartClick"
          autoresize
          style="width: 100%; height: 95%;"
        />
      </div>
    </n-card>

    <Teleport to="body">
      <div v-if="isChartFullscreen" class="fullscreen-overlay">
        <div>
          <n-button @click="toggleFullscreen">
              {{ isChartFullscreen ? '退出全屏' : '全屏显示' }}
          </n-button>
        </div>
        <div class="fullscreen-content">
          <div>
          <n-radio-group v-model:value="chartClickMode">
          <n-radio-button value="expand">{{ TEXT.clickToExpand }}</n-radio-button>
          <n-radio-button value="dig">{{ TEXT.clickToDig }}</n-radio-button>            
          <n-radio-button value="multi">{{ TEXT.multiChoice }}</n-radio-button>
          <n-radio-button value="darg">{{ TEXT.multiDarg }}</n-radio-button>
          </n-radio-group>
          <n-button @click="handleToParentPath">{{TEXT.toParentPath}}</n-button>
          <n-button @click="handleToChoiceAll">{{TEXT.toChoiceAll}}</n-button>
          <n-button @click="handleToChoiceInvert">{{TEXT.toChoiceInvert}}</n-button>
          <n-button @click="handleToChoiceNull">{{TEXT.toChoiceNull}}</n-button>
          <n-button @click="handleToPackAll">{{TEXT.toPackAll}}</n-button>
          <n-button @click="choiceAiAdvise">{{TEXT.toAiAdvise}}</n-button>
          </div>
           <v-chart
            ref="chartRef"
            :option="chartOption"
            @click="handleChartClick"
            autoresize
            style="width: 100%; height: 95%;"
          />
        </div>
      </div>
    </Teleport>

    <AppOverviewSection
      :report-key="reportKey"
      :selected-app-key="selectedApp?.key ?? null"
      @select-app="handleSelectApp"
      @clear-app="handleClearSelectedApp"
    />

    <div class="results-dual-grid">
      <n-card class="surface-card interactive-card">
        <template #header>
          <div class="section-head">
            <div>
              <div class="section-head__title">{{ TEXT.scanModules }}</div>
              <div class="section-head__desc">把识别到的问题按模块拆开，便于用户判断优先级。</div>
            </div>
          </div>
        </template>

        <n-grid cols="1 s:2" responsive="screen" :x-gap="12" :y-gap="12">
          <n-gi v-for="item in moduleCards" :key="item.kind">
            <div class="results-module-card">
              <n-statistic :label="item.label" :value="item.itemCount" />
              <n-text depth="3" class="results-module-card__desc">
                {{ item.description }}
              </n-text>
              <n-tag size="small" round>{{ formatBytes(item.totalSize) }}</n-tag>
            </div>
          </n-gi>
        </n-grid>
      </n-card>

      <n-card class="surface-card interactive-card">
        <template #header>
          <div class="section-head">
            <div>
              <div class="section-head__title">{{ TEXT.typeDistribution }}</div>
              <div class="section-head__desc">快速识别这次扫描里最占空间的文件类型。</div>
            </div>
          </div>
        </template>
        <v-chart :option="typeChartOption" style="height: 320px" autoresize />
      </n-card>
    </div>

    <n-card v-if="filteredLargeFiles.length > 0" class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.largeFiles }}</div>
            <div class="section-head__desc">先处理体积大的内容，通常最容易带来明显的空间回收。</div>
          </div>
        </div>
      </template>
      <n-space vertical :size="12">
        <n-alert type="info">{{ TEXT.sectionLimited }}</n-alert>
        <n-data-table
          :columns="fileColumns"
          :data="filteredLargeFiles"
          :max-height="320"
          size="small"
          :bordered="false"
        />
      </n-space>
    </n-card>

    <n-card v-if="filteredTemporaryFiles.length > 0" class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.temporaryFiles }}</div>
            <div class="section-head__desc">临时内容通常风险较低，但仍建议用户先复核后执行。</div>
          </div>
        </div>
      </template>
      <n-space vertical :size="12">
        <n-alert type="info">{{ TEXT.sectionLimited }}</n-alert>
        <n-data-table
          :columns="fileColumns"
          :data="filteredTemporaryFiles"
          :max-height="320"
          size="small"
          :bordered="false"
        />
      </n-space>
    </n-card>

    <n-card v-if="filteredArchiveFiles.length > 0" class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.archiveFiles }}</div>
            <div class="section-head__desc">适合归档、搬迁或确认后删除的压缩包与安装包。</div>
          </div>
        </div>
      </template>
      <n-space vertical :size="12">
        <n-alert type="info">{{ TEXT.sectionLimited }}</n-alert>
        <n-data-table
          :columns="fileColumns"
          :data="filteredArchiveFiles"
          :max-height="320"
          size="small"
          :bordered="false"
        />
      </n-space>
    </n-card>

    <n-card
      v-if="filteredDedupGroups.length > 0 || report.dedupPending || report.dedupError"
      class="surface-card interactive-card"
    >
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.duplicateGroups }}</div>
            <div class="section-head__desc">重复组按建议标签展示，方便手动决定保留哪一份。</div>
          </div>
        </div>
      </template>
      <n-space vertical :size="12">
        <n-alert v-if="report.dedupPending" type="info">
          {{ report.dedupMessage || TEXT.dedupPendingDefault }}
        </n-alert>
        <n-alert v-if="report.dedupError" type="error">
          {{ report.dedupError }}
        </n-alert>
        <n-alert v-if="report.dedup.truncated" type="info">
          {{ TEXT.duplicateGroupsLimited }}
        </n-alert>
        <div v-if="filteredDedupGroups.length > 0" class="results-duplicate-grid">
          <n-card
            v-for="(group, idx) in filteredDedupGroups"
            :key="group.hash"
            size="small"
            embedded
            class="results-duplicate-card"
            :title="`${TEXT.groupPrefix} ${idx + 1} ${TEXT.groupSuffix} (${formatBytes(group.totalSize)})`"
          >
            <n-space vertical :size="8">
              <div v-for="file in group.files" :key="file.path" class="results-duplicate-file">
                <n-tag :type="duplicateTagType(file.path)" size="small" round>
                  {{ duplicateTagLabel(file.path) }}
                </n-tag>
                <n-text class="results-duplicate-file__path">
                  {{ file.path }}
                </n-text>
              </div>
            </n-space>
          </n-card>
        </div>
        <n-empty
          v-else-if="!report.dedupPending"
          description="当前还没有可展示的重复文件结果。"
        />
      </n-space>
    </n-card>

    <n-button type="primary" @click="goToCleanup" class="results-primary-action">
      {{ TEXT.goCleanup }}
    </n-button>

    <n-modal v-model:show="aiInsightVisible" style="width: min(720px, calc(100vw - 32px))">
      <n-card :title="TEXT.aiInspectTitle" :bordered="false" size="small" role="dialog" aria-modal="true">
        <n-space vertical :size="12">
          <n-text depth="3">{{ TEXT.aiInspectHint }}</n-text>
          <n-text style="word-break: break-all">{{ selectedAiPath }}</n-text>

          <n-alert v-if="selectedAiInsightError" type="error" :title="TEXT.aiInspectFailed">
            {{ selectedAiInsightError }}
          </n-alert>

          <template v-else-if="selectedAiInsightPending">
            <n-text>后台正在解读这个项目，你可以先关闭窗口继续查看其它内容，稍后再回来查看结果。</n-text>
          </template>

          <template v-else-if="selectedAiInsight">
            <n-alert
              v-if="selectedAiInsight.usedFallback"
              type="warning"
              :title="TEXT.aiInspectFallbackTitle"
            >
              <div>{{ selectedAiInsight.fallbackReason || "-" }}</div>
            </n-alert>

            <n-alert
              v-else-if="!selectedAiInsight.remoteAttempted && selectedAiInsight.source === 'local_rules'"
              type="warning"
              :title="TEXT.aiInspectLocalOnlyTitle"
            >
              当前只显示了本地规则分析结果。如需 AI 智能解读，请先在设置页面配置 API Key。
            </n-alert>

            <n-alert
              v-else-if="selectedAiInsight.source.startsWith('remote')"
              type="success"
              :title="TEXT.aiInspectRemoteSuccess"
            >
              {{ formatSourceLabel(selectedAiInsight.source) }}
            </n-alert>

            <n-space>
              <n-tag size="small" type="default">
                {{ aiTargetKindLabel(selectedAiInsight.targetKind) }}
              </n-tag>
              <n-tag size="small" :type="selectedAiInsight.source.startsWith('remote') ? 'info' : 'default'">
                {{ formatSourceLabel(selectedAiInsight.source) }}
              </n-tag>
              <n-tag size="small" :type="actionTagType(selectedAiInsight.suggestedAction)">
                {{ actionLabel(selectedAiInsight.suggestedAction) }}
              </n-tag>
              <n-tag size="small" :type="riskTagType(selectedAiInsight.risk)">
                {{ riskLabel(selectedAiInsight.risk) }}
              </n-tag>
            </n-space>

            <n-card size="small" embedded>
              <n-text depth="3">
                {{ shouldShowSeparateReasonCard(selectedAiInsight) ? TEXT.aiInspectSummary : TEXT.aiInspectReason }}
              </n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ selectedAiInsight.summary }}
              </n-text>
            </n-card>

            <n-card v-if="shouldShowSeparateReasonCard(selectedAiInsight)" size="small" embedded>
              <n-text depth="3">{{ TEXT.aiInspectReason }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ selectedAiInsight.reason }}
              </n-text>
            </n-card>

            <n-card v-if="selectedAiInsight.usedFallback && selectedAiInsight.fallbackReason" size="small" embedded>
              <n-text depth="3">{{ TEXT.aiInspectFallbackReason }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ selectedAiInsight.fallbackReason }}
              </n-text>
            </n-card>
          </template>
        </n-space>

        <template #footer>
          <n-space justify="end">
            <n-button @click="aiInsightVisible = false">{{ TEXT.close }}</n-button>
            <n-button
              type="primary"
              secondary
              :loading="selectedAiInsightPending"
              :disabled="!selectedAiPath"
              @click="retrySelectedFileInsight"
            >
              {{ TEXT.retry }}
            </n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>
  </div>
</template>

<style scoped>
.results-page {
  gap: 18px;
}

.results-hero-kicker {
  margin-bottom: 8px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--accent);
}

.results-root-panel {
  line-height: 1.7;
}

.results-root-value {
  margin-top: 6px;
  word-break: break-all;
  font-weight: 700;
  color: var(--text-strong);
}

.results-app-alert {
  box-shadow: var(--shadow-soft);
}

.results-filter-bar {
  position: sticky;
  top: 0;
  z-index: 2;
  align-items: center;
  box-shadow: 0 10px 24px rgba(15, 23, 42, 0.04);
}

.results-filter-bar__search {
  flex: 1 1 360px;
}

.results-filter-bar__select {
  width: 240px;
}

.results-dual-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(320px, 0.8fr);
  gap: 18px;
}

.results-module-card {
  position: relative;
  height: 100%;
  padding: 16px;
  border-radius: 18px;
  border: 1px solid var(--border-soft);
  background: linear-gradient(135deg, #ffffff 0%, #f8fbff 100%);
  overflow: hidden;
}

:root.dark .results-module-card {
  background: linear-gradient(180deg, rgba(30, 41, 59, 0.96) 0%, rgba(15, 23, 42, 0.98) 100%);
  border: 1px solid rgba(71, 85, 105, 0.78);
  box-shadow: 0 16px 34px rgba(0, 0, 0, 0.28);
}

.results-module-card::after {
  content: "";
  position: absolute;
  right: -26px;
  top: -20px;
  width: 92px;
  height: 92px;
  border-radius: 999px;
  background: radial-gradient(circle, rgba(59, 130, 246, 0.08), rgba(59, 130, 246, 0));
}

.results-module-card__desc {
  display: block;
  margin: 10px 0 12px;
  line-height: 1.7;
}

:root.dark .results-module-card__desc {
  color: #cbd5e1;
}

.results-duplicate-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 12px;
}

.results-duplicate-card {
  border-radius: 18px;
  box-shadow: inset 0 0 0 1px rgba(230, 235, 243, 0.75);
}

.results-duplicate-file {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.results-duplicate-file__path {
  word-break: break-all;
  line-height: 1.6;
}

.results-primary-action {
  width: 100%;
  height: 52px;
  border-radius: 18px;
}

.fullscreen-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background-color: var(--app-bg);
  z-index: 9999;
  box-sizing: border-box;
}

.fullscreen-content {
  width: 100%;
  height: 100%;
  background-color: var(--app-bg);
}

@media (max-width: 1024px) {
  .results-dual-grid {
    grid-template-columns: 1fr;
  }

  .results-filter-bar__select {
    width: 100%;
  }
}
</style>

