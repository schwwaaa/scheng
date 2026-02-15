{
  "patcher": {
    "fileversion": 1,
    "appversion": {
      "major": 8,
      "minor": 6,
      "revision": 0,
      "architecture": "x64",
      "modernui": 1
    },
    "classnamespace": "box",
    "rect": [
      0.0,
      0.0,
      800.0,
      600.0
    ],
    "bglocked": 0,
    "openinpresentation": 0,
    "default_fontsize": 12.0,
    "default_fontface": 0,
    "default_fontname": "Arial",
    "gridonopen": 0,
    "gridsize": [
      15.0,
      15.0
    ],
    "boxes": [
      {
        "box": {
          "id": "obj-1",
          "maxclass": "comment",
          "patching_rect": [
            20,
            20,
            600,
            20
          ],
          "text": "scheng-sdk feedback_pingpong OSC controls (send to 127.0.0.1:9000)",
          "numinlets": 1,
          "numoutlets": 0
        }
      },
      {
        "box": {
          "id": "obj-2",
          "maxclass": "newobj",
          "patching_rect": [
            20,
            50,
            200,
            22
          ],
          "text": "udpsend 127.0.0.1 9000",
          "numinlets": 1,
          "numoutlets": 0
        }
      },
      {
        "box": {
          "id": "obj-3",
          "maxclass": "comment",
          "patching_rect": [
            20,
            90,
            120,
            20
          ],
          "text": "u_speed",
          "numinlets": 1,
          "numoutlets": 0
        }
      },
      {
        "box": {
          "id": "obj-4",
          "maxclass": "flonum",
          "patching_rect": [
            150,
            90,
            80,
            22
          ],
          "numinlets": 1,
          "numoutlets": 2
        }
      },
      {
        "box": {
          "id": "obj-5",
          "maxclass": "message",
          "patching_rect": [
            240,
            90,
            50,
            22
          ],
          "text": "0.2",
          "numinlets": 2,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-6",
          "maxclass": "newobj",
          "patching_rect": [
            300,
            90,
            180,
            22
          ],
          "text": "prepend /param/u_speed",
          "numinlets": 1,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-7",
          "maxclass": "comment",
          "patching_rect": [
            20,
            122,
            120,
            20
          ],
          "text": "u_amount",
          "numinlets": 1,
          "numoutlets": 0
        }
      },
      {
        "box": {
          "id": "obj-8",
          "maxclass": "flonum",
          "patching_rect": [
            150,
            122,
            80,
            22
          ],
          "numinlets": 1,
          "numoutlets": 2
        }
      },
      {
        "box": {
          "id": "obj-9",
          "maxclass": "message",
          "patching_rect": [
            240,
            122,
            50,
            22
          ],
          "text": "0.3",
          "numinlets": 2,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-10",
          "maxclass": "newobj",
          "patching_rect": [
            300,
            122,
            180,
            22
          ],
          "text": "prepend /param/u_amount",
          "numinlets": 1,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-11",
          "maxclass": "comment",
          "patching_rect": [
            20,
            154,
            120,
            20
          ],
          "text": "u_shift",
          "numinlets": 1,
          "numoutlets": 0
        }
      },
      {
        "box": {
          "id": "obj-12",
          "maxclass": "flonum",
          "patching_rect": [
            150,
            154,
            80,
            22
          ],
          "numinlets": 1,
          "numoutlets": 2
        }
      },
      {
        "box": {
          "id": "obj-13",
          "maxclass": "message",
          "patching_rect": [
            240,
            154,
            50,
            22
          ],
          "text": "0.02",
          "numinlets": 2,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-14",
          "maxclass": "newobj",
          "patching_rect": [
            300,
            154,
            180,
            22
          ],
          "text": "prepend /param/u_shift",
          "numinlets": 1,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-15",
          "maxclass": "comment",
          "patching_rect": [
            20,
            186,
            120,
            20
          ],
          "text": "u_mix",
          "numinlets": 1,
          "numoutlets": 0
        }
      },
      {
        "box": {
          "id": "obj-16",
          "maxclass": "flonum",
          "patching_rect": [
            150,
            186,
            80,
            22
          ],
          "numinlets": 1,
          "numoutlets": 2
        }
      },
      {
        "box": {
          "id": "obj-17",
          "maxclass": "message",
          "patching_rect": [
            240,
            186,
            50,
            22
          ],
          "text": "0.9",
          "numinlets": 2,
          "numoutlets": 1
        }
      },
      {
        "box": {
          "id": "obj-18",
          "maxclass": "newobj",
          "patching_rect": [
            300,
            186,
            180,
            22
          ],
          "text": "prepend /param/u_mix",
          "numinlets": 1,
          "numoutlets": 1
        }
      }
    ],
    "lines": [
      {
        "patchline": {
          "source": [
            "obj-5",
            0
          ],
          "destination": [
            "obj-4",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-4",
            0
          ],
          "destination": [
            "obj-6",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-6",
            0
          ],
          "destination": [
            "obj-2",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-9",
            0
          ],
          "destination": [
            "obj-8",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-8",
            0
          ],
          "destination": [
            "obj-10",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-10",
            0
          ],
          "destination": [
            "obj-2",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-13",
            0
          ],
          "destination": [
            "obj-12",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-12",
            0
          ],
          "destination": [
            "obj-14",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-14",
            0
          ],
          "destination": [
            "obj-2",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-17",
            0
          ],
          "destination": [
            "obj-16",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-16",
            0
          ],
          "destination": [
            "obj-18",
            0
          ]
        }
      },
      {
        "patchline": {
          "source": [
            "obj-18",
            0
          ],
          "destination": [
            "obj-2",
            0
          ]
        }
      }
    ],
    "dependency_cache": [],
    "autosave": 0
  }
}