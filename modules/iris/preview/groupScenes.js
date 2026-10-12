.pragma library

var sectionScenes = ({ dock: "dock", player: "player", desktop: "widgets", sidebars: "panels", controlCenter: "controlCenter", spotlight: "spotlight", bubbles: "bubbles",
    bar: "islandEdge", appearance: "light", motion: "motion", lock: "lock", frameMusic: "frame", notifications: "feedback", sound: "feedback" })

var groupScenes = ({
    "bar/Visibility": "islandReserve", "bar/Interaction": "islandInteraction",
    "bar/Shape": "shapes", "bar/Layout": "islandEdge", "bar/Bar": "barZones",
    "appearance/Light": "light", "appearance/Shape": "shapes", "appearance/Colour theme": "glass", "appearance/Scheme": "glass", "appearance/Dark look": "glass", "appearance/Ink look": "glass", "appearance/Light look": "glass", "appearance/Corners per surface": "fusion", "appearance/Glass": "glass", "appearance/Edges": "glass",
    "appearance/Menus": "menus", "appearance/Settings": "settings",
    "appearance/Material": "glass", "appearance/Material per surface": "glass", "appearance/Look": "fusion",
    "appearance/Adaptive": "fusion", "appearance/Accent": "controlCenter", "appearance/Highlight": "controlCenter",
    "appearance/Faces": "typography", "appearance/Text": "typography", "appearance/Frame": "frame",
    "desktop/Desktop menu": "menus",
    "motion/Motion": "motion", "motion/Curve": "motion", "motion/Timing": "motion",
    "motion/Per surface": "motion", "motion/Style per surface": "motion", "motion/Touch": "motion",
    "frameMusic/On the edges": "frame", "frameMusic/Frame response": "frame", "frameMusic/Finish": "frame",
    "lock/Scene": "lock", "lock/Type": "lock", "lock/Clock": "lock", "lock/At a glance": "lock",
    "lock/Now playing": "lock", "lock/Activity": "lock", "lock/Sign in": "lock", "lock/Status": "lock",
    "bar/Desktop page": "islandPage", "bar/Pages": "islandPage", "bar/Player page": "islandPage",
    "bubbles/Behaviour": "bubbles", "bubbles/Floating": "bubbles", "bubbles/On the contour": "bubbles", "bubbles/Size": "bubbles",
    "bubbles/Cards": "cards", "bubbles/Card contents": "cards", "bubbles/Joining": "joining", "bubbles/Tray": "tray",
    "dock/Visibility": "dock", "dock/Icons": "dock", "dock/Look": "dock",
    "desktop/Widgets": "widgets", "desktop/Overview backdrop": "backdrop", "desktop/Wallpaper gallery": "gallery",
    "spotlight/Spotlight": "spotlight", "controlCenter/Control Center": "controlCenter",
    "sidebars/Panel look": "panels",
    "notifications/Banners": "feedback", "notifications/Notifications": "feedback", "sound/Feedback": "feedback",
    "player/Player": "player", "player/Bubble": "player"
})

function sceneFor(section, group) {
    if (group.length === 0)
        return sectionScenes[section] ?? ""
    return groupScenes[section + "/" + group] ?? ""
}
