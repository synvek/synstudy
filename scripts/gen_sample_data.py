#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generate a small SAMPLE dataset so the build pipeline can run offline.

This writes:
  data/ecdict.csv          -- ECDICT-format master dictionary (subset)
  data/dicts/xiaoxue.json  -- qwerty-learner-format list (小学)
  data/dicts/tem4.json     -- qwerty-learner-format list (专四)
  data/dicts/tem8.json     -- qwerty-learner-format list (专八)

The sample is only for development/testing/demo. For production, replace these
files with the real datasets (see README):
  * ECDICT          https://github.com/skywind3000/ECDICT   (ecdict.csv, MIT)
  * qwerty-learner  https://github.com/RealKai42/qwerty-learner (public/dicts/*.json)

Entry tuple layout:
  (word, phonetic, translation_zh, definition_en, pos, category_key)
A word may appear once per category; the first occurrence supplies its fields.
"""
import csv
import json
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "data")
DICTS = os.path.join(DATA, "dicts")

# category_key -> (display_name, source)
#   source == "tag"  : derived from the ECDICT `tag` column in build_dict_db.py
#   source == "json" : derived from a qwerty-learner style JSON list
CATEGORIES = [
    ("xiaoxue",  "小学",        "json"),
    ("chuzhong", "初中",        "tag"),
    ("gaozhong", "高中",        "tag"),
    ("cet4",     "大学英语四级", "tag"),
    ("cet6",     "大学英语六级", "tag"),
    ("tem4",     "英语专业四级", "json"),
    ("tem8",     "英语专业八级", "json"),
    ("toefl",    "托福",        "tag"),
    ("gre",      "GRE",        "tag"),
]

# ECDICT `tag` value used for each tag-sourced category.
CAT_TAG = {"chuzhong": "zk", "gaozhong": "gk", "cet4": "cet4",
           "cet6": "cet6", "toefl": "toefl", "gre": "gre"}

# Approximate frequency tier per category (lower == more common).
TIER = {"xiaoxue": 800, "chuzhong": 1500, "gaozhong": 3000, "cet4": 4500,
        "cet6": 6500, "tem4": 7000, "toefl": 9500, "gre": 15000, "tem8": 16000}

# --- Sample vocabulary -------------------------------------------------------
ENTRIES = [
    # 小学 (xiaoxue)
    ("apple", "/ˈæpl/", "n. 苹果", "a round fruit with red or green skin", "n:100", "xiaoxue"),
    ("book", "/bʊk/", "n. 书；v. 预订", "a set of printed pages bound together", "n:70/v:30", "xiaoxue"),
    ("cat", "/kæt/", "n. 猫", "a small domestic animal", "n:100", "xiaoxue"),
    ("dog", "/dɒɡ/", "n. 狗", "an animal that barks", "n:100", "xiaoxue"),
    ("egg", "/eɡ/", "n. 蛋；鸡蛋", "a round object laid by a bird", "n:100", "xiaoxue"),
    ("fish", "/fɪʃ/", "n. 鱼", "an animal that lives in water", "n:80/v:20", "xiaoxue"),
    ("girl", "/ɡɜːl/", "n. 女孩", "a female child", "n:100", "xiaoxue"),
    ("hand", "/hænd/", "n. 手", "the body part at the end of the arm", "n:80/v:20", "xiaoxue"),
    ("juice", "/dʒuːs/", "n. 果汁", "the liquid from fruit", "n:100", "xiaoxue"),
    ("kite", "/kaɪt/", "n. 风筝", "a toy that flies in the wind", "n:100", "xiaoxue"),
    ("lion", "/ˈlaɪən/", "n. 狮子", "a large wild cat", "n:100", "xiaoxue"),
    ("milk", "/mɪlk/", "n. 牛奶", "a white liquid from cows", "n:80/v:20", "xiaoxue"),
    ("nose", "/nəʊz/", "n. 鼻子", "the body part used for smelling", "n:100", "xiaoxue"),
    ("orange", "/ˈɒrɪndʒ/", "n. 橙子；adj. 橙色", "a round citrus fruit", "n:90/adj:10", "xiaoxue"),
    ("pen", "/pen/", "n. 钢笔", "a tool for writing with ink", "n:100", "xiaoxue"),
    ("rabbit", "/ˈræbɪt/", "n. 兔子", "a small animal with long ears", "n:100", "xiaoxue"),
    ("sun", "/sʌn/", "n. 太阳", "the star that gives light", "n:100", "xiaoxue"),
    ("tree", "/triː/", "n. 树", "a tall plant with a trunk", "n:100", "xiaoxue"),
    ("water", "/ˈwɔːtə/", "n. 水", "a clear liquid essential for life", "n:80/v:20", "xiaoxue"),
    ("window", "/ˈwɪndəʊ/", "n. 窗户", "an opening in a wall with glass", "n:100", "xiaoxue"),

    # 初中 (chuzhong)
    ("student", "/ˈstjuːdnt/", "n. 学生", "a person who studies at a school", "n:100", "chuzhong"),
    ("teacher", "/ˈtiːtʃə/", "n. 教师", "a person who teaches", "n:100", "chuzhong"),
    ("family", "/ˈfæməli/", "n. 家庭；家人", "a group of related people", "n:100", "chuzhong"),
    ("friend", "/frend/", "n. 朋友", "a person you like and trust", "n:100", "chuzhong"),
    ("school", "/skuːl/", "n. 学校", "a place where children learn", "n:100", "chuzhong"),
    ("homework", "/ˈhəʊmwɜːk/", "n. 家庭作业", "work done at home for school", "n:100", "chuzhong"),
    ("weather", "/ˈweðə/", "n. 天气", "the condition of the atmosphere", "n:100", "chuzhong"),
    ("hospital", "/ˈhɒspɪtl/", "n. 医院", "a place where sick people are treated", "n:100", "chuzhong"),
    ("library", "/ˈlaɪbrəri/", "n. 图书馆", "a place with books to read or borrow", "n:100", "chuzhong"),
    ("mountain", "/ˈmaʊntən/", "n. 山；山脉", "a very high hill", "n:100", "chuzhong"),
    ("science", "/ˈsaɪəns/", "n. 科学", "the study of the natural world", "n:100", "chuzhong"),
    ("history", "/ˈhɪstri/", "n. 历史", "the study of past events", "n:100", "chuzhong"),
    ("important", "/ɪmˈpɔːtnt/", "adj. 重要的", "having great value or influence", "adj:100", "chuzhong"),
    ("different", "/ˈdɪfrənt/", "adj. 不同的", "not the same", "adj:100", "chuzhong"),
    ("remember", "/rɪˈmembə/", "v. 记得；想起", "to keep in memory", "v:100", "chuzhong"),
    ("understand", "/ˌʌndəˈstænd/", "v. 理解；明白", "to know the meaning of", "v:100", "chuzhong"),
    ("beautiful", "/ˈbjuːtɪfl/", "adj. 美丽的", "very pleasing", "adj:100", "chuzhong"),
    ("together", "/təˈɡeðə/", "adv. 一起", "with each other", "adv:100", "chuzhong"),
    ("dangerous", "/ˈdeɪndʒərəs/", "adj. 危险的", "able to cause harm", "adj:100", "chuzhong"),
    ("healthy", "/ˈhelθi/", "adj. 健康的", "in good health", "adj:100", "chuzhong"),

    # 高中 (gaozhong)
    ("knowledge", "/ˈnɒlɪdʒ/", "n. 知识", "information and understanding", "n:100", "gaozhong"),
    ("experience", "/ɪkˈspɪəriəns/", "n. 经验；经历", "knowledge gained from doing things", "n:80/v:20", "gaozhong"),
    ("environment", "/ɪnˈvaɪrənmənt/", "n. 环境", "the natural world around us", "n:100", "gaozhong"),
    ("society", "/səˈsaɪəti/", "n. 社会", "people living together in a community", "n:100", "gaozhong"),
    ("technology", "/tekˈnɒlədʒi/", "n. 技术；科技", "the application of science", "n:100", "gaozhong"),
    ("government", "/ˈɡʌvənmənt/", "n. 政府", "the group governing a country", "n:100", "gaozhong"),
    ("education", "/ˌedʒuˈkeɪʃn/", "n. 教育", "the process of teaching and learning", "n:100", "gaozhong"),
    ("culture", "/ˈkʌltʃə/", "n. 文化", "the customs of a society", "n:100", "gaozhong"),
    ("communicate", "/kəˈmjuːnɪkeɪt/", "v. 交流；沟通", "to share information", "v:100", "gaozhong"),
    ("independent", "/ˌɪndɪˈpendənt/", "adj. 独立的", "not controlled by others", "adj:100", "gaozhong"),
    ("opportunity", "/ˌɒpəˈtjuːnəti/", "n. 机会", "a chance to do something", "n:100", "gaozhong"),
    ("challenge", "/ˈtʃælɪndʒ/", "n. 挑战", "a difficult task", "n:70/v:30", "gaozhong"),
    ("responsibility", "/rɪˌspɒnsəˈbɪləti/", "n. 责任", "a duty to deal with something", "n:100", "gaozhong"),
    ("develop", "/dɪˈveləp/", "v. 发展；开发", "to grow or change over time", "v:100", "gaozhong"),
    ("achieve", "/əˈtʃiːv/", "v. 达到；实现", "to succeed in doing something", "v:100", "gaozhong"),
    ("encourage", "/ɪnˈkʌrɪdʒ/", "v. 鼓励", "to give support or confidence", "v:100", "gaozhong"),
    ("appreciate", "/əˈpriːʃieɪt/", "v. 欣赏；感激", "to recognize the value of", "v:100", "gaozhong"),
    ("determine", "/dɪˈtɜːmɪn/", "v. 决定；确定", "to decide or find out", "v:100", "gaozhong"),
    ("significant", "/sɪɡˈnɪfɪkənt/", "adj. 重要的；显著的", "large or important enough to matter", "adj:100", "gaozhong"),
    ("confident", "/ˈkɒnfɪdənt/", "adj. 自信的", "sure of oneself", "adj:100", "gaozhong"),

    # 四级 (cet4)
    ("ability", "/əˈbɪləti/", "n. 能力", "the power to do something", "n:100", "cet4"),
    ("access", "/ˈækses/", "n. 通道；访问", "the right or way to use something", "n:70/v:30", "cet4"),
    ("accomplish", "/əˈkʌmplɪʃ/", "v. 完成；实现", "to succeed in doing", "v:100", "cet4"),
    ("acquire", "/əˈkwaɪə/", "v. 获得；学到", "to gain or obtain", "v:100", "cet4"),
    ("adapt", "/əˈdæpt/", "v. 适应；改编", "to change to suit conditions", "v:100", "cet4"),
    ("adequate", "/ˈædɪkwət/", "adj. 足够的", "enough for a purpose", "adj:100", "cet4"),
    ("adjust", "/əˈdʒʌst/", "v. 调整", "to change slightly", "v:100", "cet4"),
    ("advance", "/ədˈvɑːns/", "v. 前进；促进", "to move forward", "v:70/n:30", "cet4"),
    ("advocate", "/ˈædvəkeɪt/", "v. 提倡；拥护", "to publicly support", "v:60/n:40", "cet4"),
    ("affect", "/əˈfekt/", "v. 影响", "to have an effect on", "v:100", "cet4"),
    ("afford", "/əˈfɔːd/", "v. 负担得起", "to have enough money for", "v:100", "cet4"),
    ("agency", "/ˈeɪdʒənsi/", "n. 代理机构", "an organization providing a service", "n:100", "cet4"),
    ("amount", "/əˈmaʊnt/", "n. 数量", "a quantity of something", "n:80/v:20", "cet4"),
    ("analyze", "/ˈænəlaɪz/", "v. 分析", "to examine in detail", "v:100", "cet4"),
    ("anticipate", "/ænˈtɪsɪpeɪt/", "v. 预期", "to expect or predict", "v:100", "cet4"),
    ("apparent", "/əˈpærənt/", "adj. 明显的", "easy to see or understand", "adj:100", "cet4"),
    ("apply", "/əˈplaɪ/", "v. 申请；应用", "to put to use", "v:100", "cet4"),
    ("approach", "/əˈprəʊtʃ/", "n. 方法；v. 接近", "a way of dealing with something", "n:60/v:40", "cet4"),
    ("appropriate", "/əˈprəʊpriət/", "adj. 适当的", "suitable for a situation", "adj:80/v:20", "cet4"),
    ("aspect", "/ˈæspekt/", "n. 方面", "one part of something", "n:100", "cet4"),

    # 六级 (cet6)
    ("abolish", "/əˈbɒlɪʃ/", "v. 废除", "to officially end a law", "v:100", "cet6"),
    ("absorb", "/əbˈzɔːb/", "v. 吸收", "to take in a substance", "v:100", "cet6"),
    ("abstract", "/ˈæbstrækt/", "adj. 抽象的", "existing in thought but not concrete", "adj:60/n:40", "cet6"),
    ("abundant", "/əˈbʌndənt/", "adj. 丰富的", "existing in large amounts", "adj:100", "cet6"),
    ("accelerate", "/əkˈseləreɪt/", "v. 加速", "to increase in speed", "v:100", "cet6"),
    ("accumulate", "/əˈkjuːmjəleɪt/", "v. 积累", "to gather over time", "v:100", "cet6"),
    ("accuse", "/əˈkjuːz/", "v. 指责；控告", "to say someone did wrong", "v:100", "cet6"),
    ("acquaint", "/əˈkweɪnt/", "v. 使熟悉", "to make someone aware of", "v:100", "cet6"),
    ("adhere", "/ədˈhɪə/", "v. 坚持；粘附", "to stick to or follow", "v:100", "cet6"),
    ("adjacent", "/əˈdʒeɪsnt/", "adj. 邻近的", "next to something", "adj:100", "cet6"),
    ("administer", "/ədˈmɪnɪstə/", "v. 管理", "to control or manage", "v:100", "cet6"),
    ("adolescent", "/ˌædəˈlesnt/", "n. 青少年", "a young person", "n:80/adj:20", "cet6"),
    ("aesthetic", "/iːsˈθetɪk/", "adj. 美学的", "concerned with beauty", "adj:70/n:30", "cet6"),
    ("aggravate", "/ˈæɡrəveɪt/", "v. 加剧", "to make worse", "v:100", "cet6"),
    ("allocate", "/ˈæləkeɪt/", "v. 分配", "to distribute resources", "v:100", "cet6"),
    ("ambiguous", "/æmˈbɪɡjuəs/", "adj. 模棱两可的", "having more than one meaning", "adj:100", "cet6"),
    ("amplify", "/ˈæmplɪfaɪ/", "v. 放大；增强", "to increase in strength", "v:100", "cet6"),
    ("anonymous", "/əˈnɒnɪməs/", "adj. 匿名的", "not named", "adj:100", "cet6"),
    ("apprehend", "/ˌæprɪˈhend/", "v. 逮捕；理解", "to arrest or grasp", "v:100", "cet6"),
    ("arbitrary", "/ˈɑːbɪtrəri/", "adj. 任意的", "based on random choice", "adj:100", "cet6"),

    # 托福 (toefl)
    ("hypothesis", "/haɪˈpɒθəsɪs/", "n. 假说", "a proposed explanation", "n:100", "toefl"),
    ("empirical", "/ɪmˈpɪrɪkl/", "adj. 实证的", "based on observation", "adj:100", "toefl"),
    ("paradigm", "/ˈpærədaɪm/", "n. 范式", "a typical example or model", "n:100", "toefl"),
    ("infrastructure", "/ˈɪnfrəstrʌktʃə/", "n. 基础设施", "basic systems of a society", "n:100", "toefl"),
    ("biodiversity", "/ˌbaɪəʊdaɪˈvɜːsəti/", "n. 生物多样性", "variety of life", "n:100", "toefl"),
    ("ecosystem", "/ˈiːkəʊsɪstəm/", "n. 生态系统", "a biological community", "n:100", "toefl"),
    ("sediment", "/ˈsedɪmənt/", "n. 沉积物", "matter that settles at the bottom", "n:100", "toefl"),
    ("erosion", "/ɪˈrəʊʒn/", "n. 侵蚀", "gradual wearing away", "n:100", "toefl"),
    ("glacier", "/ˈɡlæsiə/", "n. 冰川", "a slow-moving mass of ice", "n:100", "toefl"),
    ("metabolism", "/məˈtæbəlɪzəm/", "n. 新陈代谢", "chemical processes in an organism", "n:100", "toefl"),
    ("photosynthesis", "/ˌfəʊtəʊˈsɪnθəsɪs/", "n. 光合作用", "process by which plants make food", "n:100", "toefl"),
    ("organism", "/ˈɔːɡənɪzəm/", "n. 生物体", "an individual living thing", "n:100", "toefl"),
    ("tectonic", "/tekˈtɒnɪk/", "adj. 地壳的", "relating to the earth's crust", "adj:100", "toefl"),
    ("atmosphere", "/ˈætməsfɪə/", "n. 大气；气氛", "the air around the earth", "n:100", "toefl"),
    ("precipitation", "/prɪˌsɪpɪˈteɪʃn/", "n. 降水", "rain, snow, etc.", "n:100", "toefl"),
    ("condensation", "/ˌkɒndenˈseɪʃn/", "n. 凝结", "change from vapor to liquid", "n:100", "toefl"),
    ("evaporation", "/ɪˌvæpəˈreɪʃn/", "n. 蒸发", "change from liquid to vapor", "n:100", "toefl"),
    ("germination", "/ˌdʒɜːmɪˈneɪʃn/", "n. 发芽", "the process of a seed sprouting", "n:100", "toefl"),
    ("terrestrial", "/təˈrestriəl/", "adj. 陆地的", "relating to land", "adj:100", "toefl"),
    ("aquatic", "/əˈkwætɪk/", "adj. 水生的", "relating to water", "adj:100", "toefl"),

    # GRE (gre)
    ("ubiquitous", "/juːˈbɪkwɪtəs/", "adj. 无所不在的", "present everywhere", "adj:100", "gre"),
    ("ephemeral", "/ɪˈfemərəl/", "adj. 短暂的", "lasting a very short time", "adj:100", "gre"),
    ("esoteric", "/ˌesəˈterɪk/", "adj. 深奥的", "understood by few", "adj:100", "gre"),
    ("obsequious", "/əbˈsiːkwiəs/", "adj. 谄媚的", "too eager to obey", "adj:100", "gre"),
    ("sycophant", "/ˈsɪkəfænt/", "n. 马屁精", "a person who flatters", "n:100", "gre"),
    ("magnanimous", "/mæɡˈnænɪməs/", "adj. 宽宏大量的", "generous and forgiving", "adj:100", "gre"),
    ("perfunctory", "/pəˈfʌŋktəri/", "adj. 敷衍的", "done without care", "adj:100", "gre"),
    ("recalcitrant", "/rɪˈkælsɪtrənt/", "adj. 桀骜不驯的", "resisting authority", "adj:100", "gre"),
    ("sagacious", "/səˈɡeɪʃəs/", "adj. 睿智的", "having keen judgment", "adj:100", "gre"),
    ("tenacious", "/təˈneɪʃəs/", "adj. 坚韧的", "holding firmly", "adj:100", "gre"),
    ("verbose", "/vɜːˈbəʊs/", "adj. 冗长的", "using too many words", "adj:100", "gre"),
    ("laconic", "/ləˈkɒnɪk/", "adj. 简洁的", "using few words", "adj:100", "gre"),
    ("alacrity", "/əˈlækrəti/", "n. 欣然；敏捷", "eager willingness", "n:100", "gre"),
    ("ameliorate", "/əˈmiːliəreɪt/", "v. 改善", "to make better", "v:100", "gre"),
    ("assuage", "/əˈsweɪdʒ/", "v. 缓和", "to make less intense", "v:100", "gre"),
    ("avarice", "/ˈævərɪs/", "n. 贪婪", "extreme greed", "n:100", "gre"),
    ("capricious", "/kəˈprɪʃəs/", "adj. 反复无常的", "changing suddenly", "adj:100", "gre"),
    ("dearth", "/dɜːθ/", "n. 缺乏", "a shortage", "n:100", "gre"),
    ("equivocate", "/ɪˈkwɪvəkeɪt/", "v. 含糊其辞", "to use vague language", "v:100", "gre"),
    ("fastidious", "/fæˈstɪdiəs/", "adj. 一丝不苟的", "very attentive to detail", "adj:100", "gre"),

    # 专四 (tem4)
    ("abandon", "/əˈbændən/", "v. 放弃；抛弃", "to leave and not return", "v:100", "tem4"),
    ("abide", "/əˈbaɪd/", "v. 遵守；忍受", "to accept or follow", "v:100", "tem4"),
    ("abnormal", "/æbˈnɔːml/", "adj. 不正常的", "not usual", "adj:100", "tem4"),
    ("abound", "/əˈbaʊnd/", "v. 大量存在", "to exist in large numbers", "v:100", "tem4"),
    ("abrupt", "/əˈbrʌpt/", "adj. 突然的", "sudden and unexpected", "adj:100", "tem4"),
    ("absurd", "/əbˈsɜːd/", "adj. 荒谬的", "wildly unreasonable", "adj:100", "tem4"),
    ("accessible", "/əkˈsesəbl/", "adj. 可进入的；易理解的", "able to be reached", "adj:100", "tem4"),
    ("accommodate", "/əˈkɒmədeɪt/", "v. 容纳", "to provide space for", "v:100", "tem4"),
    ("accompany", "/əˈkʌmpəni/", "v. 陪伴", "to go with", "v:100", "tem4"),
    ("accord", "/əˈkɔːd/", "n. 协议", "an agreement", "n:60/v:40", "tem4"),
    ("accurate", "/ˈækjərət/", "adj. 准确的", "correct and precise", "adj:100", "tem4"),
    ("accustom", "/əˈkʌstəm/", "v. 使习惯", "to make familiar", "v:100", "tem4"),
    ("acknowledge", "/əkˈnɒlɪdʒ/", "v. 承认", "to accept as true", "v:100", "tem4"),
    ("alter", "/ˈɔːltə/", "v. 改变", "to change", "v:100", "tem4"),
    ("ambition", "/æmˈbɪʃn/", "n. 雄心", "a strong desire to succeed", "n:100", "tem4"),
    ("amend", "/əˈmend/", "v. 修正", "to make changes to improve", "v:100", "tem4"),
    ("amiable", "/ˈeɪmiəbl/", "adj. 和蔼的", "friendly and pleasant", "adj:100", "tem4"),
    ("ample", "/ˈæmpl/", "adj. 充足的", "more than enough", "adj:100", "tem4"),
    ("analogy", "/əˈnælədʒi/", "n. 类比", "a comparison between things", "n:100", "tem4"),
    ("ascend", "/əˈsend/", "v. 上升", "to go up", "v:100", "tem4"),

    # 专八 (tem8)
    ("abate", "/əˈbeɪt/", "v. 减弱", "to become less strong", "v:100", "tem8"),
    ("abdicate", "/ˈæbdɪkeɪt/", "v. 退位", "to give up power", "v:100", "tem8"),
    ("aberration", "/ˌæbəˈreɪʃn/", "n. 偏差", "a departure from what is normal", "n:100", "tem8"),
    ("abhor", "/əbˈhɔː/", "v. 憎恨", "to hate strongly", "v:100", "tem8"),
    ("abject", "/ˈæbdʒekt/", "adj. 凄惨的", "extremely bad", "adj:100", "tem8"),
    ("abnegate", "/ˈæbnɪɡeɪt/", "v. 放弃", "to renounce", "v:100", "tem8"),
    ("abominable", "/əˈbɒmɪnəbl/", "adj. 可恶的", "very unpleasant", "adj:100", "tem8"),
    ("abridge", "/əˈbrɪdʒ/", "v. 删节", "to shorten a text", "v:100", "tem8"),
    ("abrogate", "/ˈæbrəɡeɪt/", "v. 废除", "to repeal a law", "v:100", "tem8"),
    ("abscond", "/əbˈskɒnd/", "v. 潜逃", "to leave secretly", "v:100", "tem8"),
    ("abstemious", "/əbˈstiːmiəs/", "adj. 节制的", "not self-indulgent", "adj:100", "tem8"),
    ("abstruse", "/əbˈstruːs/", "adj. 难懂的", "hard to understand", "adj:100", "tem8"),
    ("accede", "/əkˈsiːd/", "v. 同意", "to agree to a request", "v:100", "tem8"),
    ("acclimate", "/ˈæklɪmeɪt/", "v. 适应", "to adapt to a climate", "v:100", "tem8"),
    ("accolade", "/ˈækəleɪd/", "n. 赞扬", "an award or praise", "n:100", "tem8"),
    ("accost", "/əˈkɒst/", "v. 搭讪", "to approach and speak to", "v:100", "tem8"),
    ("accrue", "/əˈkruː/", "v. 积累", "to accumulate over time", "v:100", "tem8"),
    ("acerbic", "/əˈsɜːbɪk/", "adj. 尖刻的", "sharp in tone", "adj:100", "tem8"),
    ("acquiesce", "/ˌækwiˈes/", "v. 默许", "to accept without protest", "v:100", "tem8"),
    ("acrimonious", "/ˌækrɪˈməʊniəs/", "adj. 尖酸的", "bitter in tone", "adj:100", "tem8"),
]

# A tiny CC-CEDICT-format sample so 汉英 (Chinese->English) lookups work offline.
# Line format:  Traditional Simplified [pin1 yin1] /gloss/gloss/
# Replace with the real data/cedict.txt (see scripts/fetch_data.sh) for production.
SAMPLE_CEDICT = [
    "蘋果 苹果 [ping2 guo3] /apple/CL:個|个[ge4]/",
    "書 书 [shu1] /book/CL:本[ben3]/to write/",
    "貓 猫 [mao1] /cat/CL:隻|只[zhi1]/",
    "狗 狗 [gou3] /dog/CL:條|条[tiao2]/",
    "水 水 [shui3] /water/CL:杯[bei1]/",
    "學生 学生 [xue2 sheng1] /student/schoolperson/CL:個|个[ge4]/",
    "老師 老师 [lao3 shi1] /teacher/CL:個|个[ge4]/",
    "學習 学习 [xue2 xi2] /to learn/to study/",
    "美麗 美丽 [mei3 li4] /beautiful/",
    "放棄 放弃 [fang4 qi4] /to renounce/to abandon/to give up/",
    "能力 能力 [neng2 li4] /ability/capability/CL:個|个[ge4]/",
    "環境 环境 [huan2 jing4] /environment/surroundings/",
    "機會 机会 [ji1 hui4] /opportunity/chance/",
    "知識 知识 [zhi1 shi5] /knowledge/intellect/",
    "計算機 计算机 [ji4 suan4 ji1] /computer/(Tw) calculator/",
]

# A tiny CEFR-J-format sample so the A1-C2 annotation works offline.
# Header mirrors the real olp-en-cefrj profile (columns: headword,pos,CEFR,...).
CEFRJ_HEADER = ["headword", "pos", "CEFR", "mode", "can-do"]
SAMPLE_CEFRJ = [
    ["apple", "noun", "A1", "written", ""],
    ["book", "noun", "A1", "written", ""],
    ["cat", "noun", "A1", "written", ""],
    ["dog", "noun", "A1", "spoken", ""],
    ["water", "noun", "A1", "written", ""],
    ["student", "noun", "A2", "written", ""],
    ["teacher", "noun", "A2", "written", ""],
    ["friend", "noun", "A1", "spoken", ""],
    ["environment", "noun", "B1", "written", ""],
    ["achieve", "verb", "B1", "written", ""],
    ["opportunity", "noun", "B2", "written", ""],
    ["abandon", "verb", "B2", "written", ""],
    ["hypothesis", "noun", "C1", "written", ""],
    ["ubiquitous", "adjective", "C2", "written", ""],
]
# C1/C2 supplement (Octanove profile) merged into the same `cefr` column.
SAMPLE_CEFRJ_C1C2 = [
    ["esoteric", "adjective", "C2", "written", ""],
    ["paradigm", "noun", "C1", "written", ""],
]

# A tiny Wiktionary (kaikki.org)-format sample so 例句 works offline.
# Each record is one JSON object per line: {word, lang, senses:[{examples:[{text,ref}]}]}.
SAMPLE_KAIKKI = [
    {"word": "apple", "lang": "English", "senses": [
        {"examples": [
            {"text": "She bit into the crisp red apple."},
            {"text": "An apple a day keeps the doctor away.", "ref": "proverb"}]}]},
    {"word": "book", "lang": "English", "senses": [
        {"examples": [
            {"text": "He read a book before bed.", "ref": "Corpus of Contemporary American English"},
            {"text": "She booked a table for two."}]}]},
    {"word": "water", "lang": "English", "senses": [
        {"examples": [
            {"text": "Pour some water into the glass."},
            {"text": "The river was polluted with dirty water."}]}]},
    {"word": "student", "lang": "English", "senses": [
        {"examples": [
            {"text": "Every student must wear an ID badge.", "ref": "New York Times"},
            {"text": "She is a graduate student in physics."}]}]},
    {"word": "achieve", "lang": "English", "senses": [
        {"examples": [
            {"text": "With hard work you can achieve your goals."},
            {"text": "The team achieved a remarkable victory."}]}]},
    {"word": "environment", "lang": "English", "senses": [
        {"examples": [
            {"text": "We must protect the environment for future generations.", "ref": "BBC"},
            {"text": "A clean environment improves productivity."}]}]},
    {"word": "abandon", "lang": "English", "senses": [
        {"examples": [
            {"text": "They had to abandon the car in the snow."},
            {"text": "He abandoned his studies for a year."}]}]},
    {"word": "ubiquitous", "lang": "English", "senses": [
        {"examples": [
            {"text": "Smartphones have become ubiquitous in modern life."},
            {"text": "Advertising is ubiquitous on the internet."}]}]},
]


def main():
    os.makedirs(DICTS, exist_ok=True)

    # word -> dict of fields, and word -> set of categories
    words = {}
    cats_of = {}
    order = []
    for word, phonetic, zh, en, pos, cat in ENTRIES:
        if word not in words:
            words[word] = {"phonetic": phonetic, "translation": zh,
                           "definition": en, "pos": pos}
            cats_of[word] = set()
            order.append(word)
        cats_of[word].add(cat)

    # --- data/ecdict.csv (ECDICT column layout) ---
    csv_path = os.path.join(DATA, "ecdict.csv")
    header = ["word", "phonetic", "definition", "translation", "pos",
              "collins", "oxford", "tag", "bnc", "frq", "exchange",
              "detail", "audio"]
    with open(csv_path, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(header)
        for word in order:
            d = words[word]
            tags = sorted(CAT_TAG[c] for c in cats_of[word] if c in CAT_TAG)
            # frequency tier = most common category this word belongs to
            tier = min(TIER[c] for c in cats_of[word])
            w.writerow([
                word, d["phonetic"], d["definition"], d["translation"],
                d["pos"], "0", "0", " ".join(tags), str(tier), str(tier),
                "", "", "",
            ])

    # --- data/dicts/*.json for json-sourced categories ---
    for key, _name, source in CATEGORIES:
        if source != "json":
            continue
        items = []
        for word in order:
            if key in cats_of[word]:
                d = words[word]
                phone = d["phonetic"].strip("/")
                items.append({
                    "name": word,
                    "trans": [d["translation"]],
                    "usphone": phone,
                    "ukphone": phone,
                })
        with open(os.path.join(DICTS, key + ".json"), "w",
                  encoding="utf-8") as f:
            json.dump(items, f, ensure_ascii=False, indent=2)

    print("Sample data written:")
    print("  {} ({} words)".format(csv_path, len(order)))
    for key, _name, source in CATEGORIES:
        if source == "json":
            n = sum(1 for wd in order if key in cats_of[wd])
            print("  data/dicts/{}.json ({} words)".format(key, n))

    # --- data/cedict.txt (CC-CEDICT format, sample subset) ---
    cedict_path = os.path.join(DATA, "cedict.txt")
    with open(cedict_path, "w", encoding="utf-8") as f:
        f.write("# CC-CEDICT (sample subset for offline demo)\n")
        f.write("#! license=https://creativecommons.org/licenses/by-sa/4.0/\n")
        for line in SAMPLE_CEDICT:
            f.write(line + "\n")
    print("  {} ({} entries)".format(cedict_path, len(SAMPLE_CEDICT)))

    # --- data/cefrj/*.csv (CEFR-J format, sample subset) ---
    cefrj_dir = os.path.join(DATA, "cefrj")
    os.makedirs(cefrj_dir, exist_ok=True)
    cefrj_path = os.path.join(cefrj_dir, "cefrj-vocabulary-profile-1.5.csv")
    with open(cefrj_path, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(CEFRJ_HEADER)
        w.writerows(SAMPLE_CEFRJ)
    oct_path = os.path.join(cefrj_dir, "octanove-vocabulary-profile-c1c2-1.0.csv")
    with open(oct_path, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(CEFRJ_HEADER)
        w.writerows(SAMPLE_CEFRJ_C1C2)
    print("  {} ({} rows) + octanove c1c2".format(cefrj_path, len(SAMPLE_CEFRJ)))

    # --- data/kaikki.en.jsonl (Wiktionary/kaikki format, sample subset) ---
    kaikki_path = os.path.join(DATA, "kaikki.en.jsonl")
    with open(kaikki_path, "w", encoding="utf-8") as f:
        for rec in SAMPLE_KAIKKI:
            f.write(json.dumps(rec, ensure_ascii=False) + "\n")
    print("  {} ({} records)".format(kaikki_path, len(SAMPLE_KAIKKI)))


if __name__ == "__main__":
    main()
